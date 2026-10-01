/** Foto produk di Azure Blob (kontainer privat): browser tak pernah bicara ke Azure, unggah dan ambil lewat server ini sehingga SAS token tak tersentuh browser dan tak tersimpan di DB. */
import { mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import sharp from 'sharp';

/** URL kontainer + SAS dibaca dari `process.env` saat request, bukan `import.meta.env` yang dibakukan Vite saat build (lihat `api.ts`). */
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

/** Tipe diterima berupa daftar tetap, bukan `image/*`, karena akhiran dari nama berkas kiriman pengguna tak layak dipercaya. */
const TIPE: Record<string, string> = {
  'image/jpeg': 'jpg',
  'image/png': 'png',
  'image/webp': 'webp',
  'image/avif': 'avif',
};

/** Awalan rute yang menyajikan foto. Bentuk `image_url` yang tersimpan. */
export const AWALAN_FOTO = '/foto/';

/** Satu ukuran saja, bukan lebar dari klien, agar pengguna login tak bisa memaksa server mengolah ulang foto berkali-kali; 480 px = kartu ±170 px di layar 3x. */
const LEBAR_KECIL = 480;
export const PARAM_KECIL = 'kecil';
const AKHIRAN_KECIL = '.kecil.webp';

const TIPE_BERKAS: Record<string, string> = Object.fromEntries(
  Object.entries(TIPE).map(([mime, akhiran]) => [akhiran, mime])
);

/** Nama blob yang boleh diminta persis bentuk `unggahFoto`; tanpa saringan ini `..%2F` bisa menarik blob mana pun di kontainer. */
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

/** Menaruh satu foto di kontainer dan mengembalikan path rute foto (bukan alamat Azure) untuk kolom `image_url`. */
export async function unggahFoto(berkas: File): Promise<string> {
  const ekstensi = TIPE[berkas.type];
  if (!ekstensi) {
    throw new GagalUnggah('Foto harus berformat JPG, PNG, WebP, atau AVIF.');
  }
  if (berkas.size > MAKS_BYTE) {
    throw new GagalUnggah('Ukuran foto maksimal 5 MB.');
  }

  // Nama berkas asli dibuang karena bisa memuat `/` atau `..` dan dua unggahan "foto.jpg" akan saling menimpa.
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
    // Badan jawaban Azure berupa XML (mis. `AuthenticationFailed`), berguna di log tapi bukan kalimat untuk pemilik toko.
    console.error('Unggah blob gagal', res.status, await res.text().catch(() => ''));
    throw new GagalUnggah('Foto gagal diunggah ke penyimpanan. Coba lagi.');
  }

  return `${AWALAN_FOTO}${nama}`;
}

/** Membuang foto yang tak dirujuk lagi; gagalnya tak dilempar karena perubahan produk sudah tersimpan dan yang tertinggal hanya satu blob yatim. */
export async function hapusFoto(imageUrl: string | null | undefined): Promise<void> {
  if (!imageUrl?.startsWith(AWALAN_FOTO)) return;
  const nama = imageUrl.slice(AWALAN_FOTO.length);
  if (!NAMA_SAH.test(nama)) return;

  const dasar = nama.slice('produk/'.length);
  for (const berkas of [dasar, `${dasar}${AKHIRAN_KECIL}`]) {
    await rm(join(direktoriCache(), berkas), { force: true }).catch(() => {});
  }

  try {
    const res = await fetch(alamatBlob(nama), { method: 'DELETE' });
    if (!res.ok && res.status !== 404) {
      console.error('Hapus blob gagal', nama, res.status);
    }
  } catch (err) {
    console.error('Hapus blob gagal', nama, err);
  }
}

/** Mengambil foto dari cache disk bila ada, kalau tidak dari kontainer; `null` berarti nama tak valid atau blob tak ada, sama-sama 404 agar penebak nama tak mendapat petunjuk. */
export async function ambilFoto(nama: string, kecil = false): Promise<Response | null> {
  if (!NAMA_SAH.test(nama)) return null;

  // Isi blob tak pernah berubah (UUID baru tiap unggahan) sehingga peramban boleh menyimpannya selamanya; `private` karena rute di balik login dan tak boleh mengendap di cache bersama.
  const respons = (data: Uint8Array, tipe: string) =>
    new Response(data as BodyInit, {
      headers: { 'Content-Type': tipe, 'Cache-Control': 'private, max-age=31536000, immutable' },
    });

  const dasar = nama.slice('produk/'.length);
  const berkas = join(direktoriCache(), kecil ? `${dasar}${AKHIRAN_KECIL}` : dasar);
  const tipeCache = kecil ? 'image/webp' : TIPE_BERKAS[dasar.slice(dasar.lastIndexOf('.') + 1)];

  const tersimpan = await readFile(berkas).catch(() => null);
  if (tersimpan) return respons(tersimpan, tipeCache);

  const res = await fetch(alamatBlob(nama));
  if (!res.ok || !res.body) {
    if (res.status !== 404) {
      console.error('Ambil blob gagal', nama, res.status);
    }
    return null;
  }

  const asli = Buffer.from(await res.arrayBuffer());
  if (!kecil) {
    await simpanCache(berkas, asli);
    return respons(asli, tipeCache);
  }

  try {
    // `rotate()` menerapkan orientasi EXIF sebelum metadata dibuang, kalau tidak foto HP tampil miring.
    const kecilWebp = await sharp(asli)
      .rotate()
      .resize({ width: LEBAR_KECIL, withoutEnlargement: true })
      .webp({ quality: 75 })
      .toBuffer();
    await simpanCache(berkas, kecilWebp);
    return respons(kecilWebp, tipeCache);
  } catch (err) {
    // Foto yang tak bisa diperkecil tampil utuh daripada kotak kosong, dan tak dicache agar pembetulan di server langsung berlaku.
    console.error('Perkecil foto gagal', nama, err);
    return respons(asli, res.headers.get('content-type') ?? 'application/octet-stream');
  }
}

/** Cache foto di disk server tanpa masa berlaku (foto tak berubah); `CACHE_DIRECTORY` dari `CacheDirectory=` systemd, satu-satunya tempat tulis di bawah `ProtectSystem=strict`. */
function direktoriCache(): string {
  return process.env.CACHE_DIRECTORY ?? join(tmpdir(), 'aj33-foto');
}

/** Gagal menulis cache tak boleh menggagalkan foto yang sudah di tangan; ditulis ke berkas sementara lalu di-rename agar pembaca tak melihat berkas setengah jadi. */
async function simpanCache(berkas: string, data: Uint8Array): Promise<void> {
  const sementara = `${berkas}.${crypto.randomUUID()}.tmp`;
  try {
    await mkdir(dirname(berkas), { recursive: true });
    await writeFile(sementara, data);
    await rename(sementara, berkas);
  } catch (err) {
    console.error('Tulis cache foto gagal', berkas, err);
    await rm(sementara, { force: true }).catch(() => {});
  }
}

/** Alamat versi kecil foto untuk kartu ±170 px; foto asli bisa 3000 px dan lebih dari 1 MB, versi ini sekitar 20 KB. */
export function fotoKecil(imageUrl: string): string {
  return `${imageUrl}?${PARAM_KECIL}`;
}
