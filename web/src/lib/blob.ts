/**
 * Foto produk di Azure Blob Storage.
 *
 * Kontainernya privat -- akun storage menolak akses anonim -- jadi browser
 * tidak pernah bicara ke Azure. Keduanya lewat server ini: form mengirim
 * berkas ke sini dan server meneruskannya ke Azure (`unggahFoto`), lalu
 * `<img>` meminta `/foto/...` dan rute itu mengambilkannya (`ambilFoto`).
 *
 * Susunan ini juga yang membuat SAS token -- kredensial tulis ke seluruh
 * kontainer, berumur beberapa hari -- tidak pernah tersentuh browser dan
 * tidak pernah ikut tersimpan di basis data. Memperbaruinya cukup mengganti
 * satu baris env; tidak ada URL tersimpan yang ikut basi.
 */

/**
 * URL kontainer lengkap dengan SAS query, mis.
 * `https://akun.blob.core.windows.net/data?sp=...&sig=...`.
 *
 * Dibaca dari `process.env` saat request berjalan, bukan dari
 * `import.meta.env` saja: Vite mengganti `import.meta.env.X` dengan nilai
 * literalnya ketika di-build, jadi SAS yang ikut ke dist adalah SAS milik
 * mesin yang mem-build -- dan memperbaruinya di server tidak berpengaruh
 * apa-apa. Lihat alasan yang sama di `api.ts`.
 */
function sasKontainer(): string {
  const url =
    (typeof process !== 'undefined' ? process.env.AZURE_BLOB_SAS_URL : undefined) ??
    import.meta.env.AZURE_BLOB_SAS_URL;
  if (!url) {
    throw new GagalUnggah('Penyimpanan foto belum disetel (AZURE_BLOB_SAS_URL).');
  }
  return url;
}

/** Batas ukuran satu foto. */
const MAKS_BYTE = 5 * 1024 * 1024;

/**
 * Tipe yang diterima, beserta akhiran berkasnya. Daftar tetap, bukan
 * `image/*`: nama blob dirakit di sini, dan akhiran yang datang dari nama
 * berkas kiriman pengguna tidak layak dipercaya.
 */
const TIPE: Record<string, string> = {
  'image/jpeg': 'jpg',
  'image/png': 'png',
  'image/webp': 'webp',
  'image/avif': 'avif',
};

/** Awalan rute yang menyajikan foto. Bentuk `image_url` yang tersimpan. */
export const AWALAN_FOTO = '/foto/';

/**
 * Nama blob yang boleh diminta lewat rute foto: persis bentuk yang dirakit
 * `unggahFoto`, tidak lebih. Nama datang dari URL, jadi ia data -- tanpa
 * saringan ini `..%2F` bisa menarik blob mana pun di kontainer, termasuk
 * yang bukan foto produk.
 */
const NAMA_SAH = /^produk\/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.(jpg|png|webp|avif)$/;

/** Gagal yang pesannya memang ditulis untuk dibaca pengguna. */
export class GagalUnggah extends Error {}

/** Alamat satu blob di kontainer, lengkap dengan SAS query. */
function alamatBlob(nama: string): URL {
  const sas = new URL(sasKontainer());
  const tujuan = new URL(sas);
  tujuan.pathname = `${sas.pathname.replace(/\/$/, '')}/${nama}`;
  return tujuan;
}

/**
 * Menaruh satu foto di kontainer dan mengembalikan alamat yang disimpan di
 * kolom `image_url` -- path rute foto, bukan alamat Azure.
 */
export async function unggahFoto(berkas: File): Promise<string> {
  const ekstensi = TIPE[berkas.type];
  if (!ekstensi) {
    throw new GagalUnggah('Foto harus berformat JPG, PNG, WebP, atau AVIF.');
  }
  if (berkas.size > MAKS_BYTE) {
    throw new GagalUnggah('Ukuran foto maksimal 5 MB.');
  }

  // Nama berkas asli dibuang seluruhnya. Nama kiriman pengguna bisa memuat
  // `/` atau `..` yang mengubah letak blob, dan dua orang yang sama-sama
  // mengunggah "foto.jpg" akan saling menimpa.
  const nama = `produk/${crypto.randomUUID()}.${ekstensi}`;

  const res = await fetch(alamatBlob(nama), {
    method: 'PUT',
    headers: {
      'x-ms-blob-type': 'BlockBlob',
      'Content-Type': berkas.type,
    },
    body: await berkas.arrayBuffer(),
  });

  if (!res.ok) {
    // Badan jawaban Azure adalah XML yang menyebut kode seperti
    // `AuthenticationFailed` -- berguna di log, tapi bukan kalimat yang
    // pantas ditampilkan ke pemilik toko.
    console.error('Unggah blob gagal', res.status, await res.text().catch(() => ''));
    throw new GagalUnggah('Foto gagal diunggah ke penyimpanan. Coba lagi.');
  }

  return `${AWALAN_FOTO}${nama}`;
}

/**
 * Membuang foto yang tidak dirujuk baris mana pun lagi -- dipanggil sesudah
 * foto produk diganti atau dikosongkan.
 *
 * Gagalnya sengaja tidak dilempar: pada titik ini perubahan produknya sudah
 * tersimpan, dan membatalkan pekerjaan pengguna karena sampah yang tertinggal
 * di kontainer adalah tukar yang salah. Yang tertinggal cuma satu blob yatim.
 */
export async function hapusFoto(imageUrl: string | null | undefined): Promise<void> {
  if (!imageUrl?.startsWith(AWALAN_FOTO)) return;
  const nama = imageUrl.slice(AWALAN_FOTO.length);
  if (!NAMA_SAH.test(nama)) return;

  try {
    const res = await fetch(alamatBlob(nama), { method: 'DELETE' });
    if (!res.ok && res.status !== 404) {
      console.error('Hapus blob gagal', nama, res.status);
    }
  } catch (err) {
    console.error('Hapus blob gagal', nama, err);
  }
}

/**
 * Mengambil satu foto dari kontainer untuk disalurkan ke browser.
 *
 * `null` berarti namanya tidak berbentuk nama foto produk, atau blobnya tidak
 * ada -- pemanggil yang memutuskan bahwa keduanya sama-sama 404. Membedakan
 * keduanya hanya memberi tahu penebak nama mana yang tebakannya mendekati.
 */
export async function ambilFoto(nama: string): Promise<Response | null> {
  if (!NAMA_SAH.test(nama)) return null;

  const res = await fetch(alamatBlob(nama));
  if (!res.ok || !res.body) {
    if (res.status !== 404) {
      console.error('Ambil blob gagal', nama, res.status);
    }
    return null;
  }

  // Isi blob tidak pernah berubah -- tiap unggahan memakai UUID baru -- jadi
  // peramban boleh menyimpannya selamanya. Tanpa ini setiap kunjungan ke
  // layar kasir menarik ulang seluruh foto lewat VPS. `private` karena
  // rutenya di balik login dan hasilnya tidak boleh mengendap di cache
  // bersama milik proxy mana pun.
  return new Response(res.body, {
    headers: {
      'Content-Type': res.headers.get('content-type') ?? 'application/octet-stream',
      'Cache-Control': 'private, max-age=31536000, immutable',
    },
  });
}
