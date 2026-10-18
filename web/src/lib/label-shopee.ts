/** Membaca PDF label pengiriman Shopee (hasil "Cetak Label" Seller Centre, satu label per halaman) menjadi isi pesanan; dijalankan di server Astro, backend memeriksa ulang isinya. */
import { getDocument, OPS } from 'pdfjs-dist/legacy/build/pdf.mjs';

/** Bentuk yang diterima `POST /orders/labels`. */
export interface LabelShopee {
  order_sn: string;
  recipient_name: string;
  address: string;
  city: string;
  district: string;
  payment_label: string;
  is_cod: boolean;
  shipping: 'instant' | 'same_day' | 'reguler';
  ship_by: string | null;
  items: { name: string; sku: string; variation: string | null; qty: number }[];
}

export interface HasilBaca {
  labels: LabelShopee[];
  /** Halaman yang tak terbaca beserta alasannya, agar owner tahu label mana yang perlu dicek manual. */
  gagal: { halaman: number; pesan: string }[];
}

/** Potongan teks dengan koordinat dari atas halaman: `y` garis dasar, `x0`/`x1` tepi kiri/kanan. */
interface Teks {
  s: string;
  x0: number;
  x1: number;
  y: number;
  ukuran: number;
}

/** Bidang isi tipis (garis bawah, garis tabel) dari atas halaman. */
interface Kotak {
  x0: number;
  x1: number;
  y0: number;
  y1: number;
}

export interface Halaman {
  lebar: number;
  teks: Teks[];
  kotak: Kotak[];
}

type Matriks = [number, number, number, number, number, number];

function kali(a: Matriks, b: number[]): Matriks {
  return [
    a[0] * b[0] + a[2] * b[1],
    a[1] * b[0] + a[3] * b[1],
    a[0] * b[2] + a[2] * b[3],
    a[1] * b[2] + a[3] * b[3],
    a[0] * b[4] + a[2] * b[5] + a[4],
    a[1] * b[4] + a[3] * b[5] + a[5],
  ];
}

function titik(m: Matriks, x: number, y: number): [number, number] {
  return [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]];
}

const OP_ISI = new Set<number>([OPS.fill, OPS.eoFill, OPS.fillStroke, OPS.eoFillStroke]);

export async function bacaHalaman(data: Uint8Array): Promise<Halaman[]> {
  const doc = await getDocument({ data, isEvalSupported: false, disableFontFace: true, verbosity: 0 })
    .promise;
  try {
    const hasil: Halaman[] = [];
    for (let n = 1; n <= doc.numPages; n++) {
      const page = await doc.getPage(n);
      const [, , lebar, tinggi] = page.view;

      const teks: Teks[] = [];
      for (const it of (await page.getTextContent()).items) {
        if (!('str' in it) || !it.str.trim()) continue;
        const [a, , , d, e, f] = it.transform as number[];
        teks.push({ s: it.str, x0: e, x1: e + it.width, y: tinggi - f, ukuran: Math.abs(d || a) });
      }

      // Koordinat path diberikan sebelum CTM, jadi save/restore/transform harus diikuti sendiri.
      const kotak: Kotak[] = [];
      const ops = await page.getOperatorList();
      let ctm: Matriks = [1, 0, 0, 1, 0, 0];
      const tumpukan: Matriks[] = [];
      let terakhir: Kotak | null = null;
      for (let i = 0; i < ops.fnArray.length; i++) {
        const fn = ops.fnArray[i];
        const args = ops.argsArray[i];
        if (fn === OPS.save) tumpukan.push(ctm);
        else if (fn === OPS.restore) ctm = tumpukan.pop() ?? ctm;
        else if (fn === OPS.transform) ctm = kali(ctm, args);
        else if (fn === OPS.constructPath) {
          const [mx0, my0, mx1, my1] = args[2] as number[];
          const [ax, ay] = titik(ctm, mx0, my0);
          const [bx, by] = titik(ctm, mx1, my1);
          terakhir = {
            x0: Math.min(ax, bx),
            x1: Math.max(ax, bx),
            y0: tinggi - Math.max(ay, by),
            y1: tinggi - Math.min(ay, by),
          };
        } else if (OP_ISI.has(fn) && terakhir) {
          kotak.push(terakhir);
          terakhir = null;
        }
      }

      hasil.push({ lebar, teks, kotak });
      page.cleanup();
    }
    return hasil;
  } finally {
    await doc.destroy();
  }
}

export async function bacaLabelPdf(data: Uint8Array): Promise<HasilBaca> {
  const hasil: HasilBaca = { labels: [], gagal: [] };
  const halaman = await bacaHalaman(data);

  halaman.forEach((h, i) => {
    if (h.teks.length === 0) return;
    try {
      hasil.labels.push(bacaLabel(h));
    } catch (err) {
      hasil.gagal.push({ halaman: i + 1, pesan: err instanceof Error ? err.message : String(err) });
    }
  });

  return hasil;
}

/** Teks dikelompokkan per baris menurut garis dasarnya; toleransi sebanding ukuran huruf karena kolom kiri dan kanan label tak selalu persis sejajar. */
function kelompokkanBaris(teks: Teks[]): Teks[][] {
  const urut = [...teks].sort((a, b) => a.y - b.y || a.x0 - b.x0);
  const baris: Teks[][] = [];
  for (const t of urut) {
    const akhir = baris.at(-1);
    if (akhir && Math.abs(akhir[0].y - t.y) <= Math.max(1.5, t.ukuran * 0.3)) akhir.push(t);
    else baris.push([t]);
  }
  return baris.map((b) => b.sort((a, c) => a.x0 - c.x0));
}

const rapi = (s: string) => s.replace(/\s+/g, ' ').trim();

function cariTeks(teks: Teks[], pola: RegExp): Teks | undefined {
  return teks.find((t) => pola.test(t.s));
}

/** Nilai di kanan sebuah judul ("Penerima: Juju"), baik dalam potongan yang sama maupun potongan berikutnya di baris itu. */
function nilaiSetelah(baris: Teks[][], judul: Teks, pola: RegExp, batasKanan = Infinity): string {
  const sisa = judul.s.replace(pola, '');
  const sebaris = baris.find((b) => b.includes(judul)) ?? [];
  const kanan = sebaris
    .filter((t) => t !== judul && t.x0 >= judul.x1 - 1 && t.x0 < batasKanan - 1)
    .map((t) => t.s);
  return rapi([sisa, ...kanan].join(' '));
}

/** Jangkauan x satu kata dalam potongan teks, diperkirakan sebanding posisi hurufnya karena pdfjs hanya memberi lebar seluruh potongan. */
function jangkauanKata(t: Teks, kata: string): [number, number] | null {
  const i = t.s.toUpperCase().indexOf(kata);
  if (i < 0) return null;
  const perHuruf = (t.x1 - t.x0) / t.s.length;
  return [t.x0 + i * perHuruf, t.x0 + (i + kata.length) * perHuruf];
}

/** Label instan mencetak "INSTANT / SAMEDAY" dan menandai yang dipilih dengan garis bawah, yang di PDF berupa bidang isi tipis tepat di bawah katanya. */
function bacaPengiriman(h: Halaman): LabelShopee['shipping'] {
  const pilihan = [
    { kata: 'INSTANT', nilai: 'instant' as const },
    { kata: 'SAMEDAY', nilai: 'same_day' as const },
  ];

  const ditemukan = pilihan.flatMap(({ kata, nilai }) =>
    h.teks.flatMap((t) => {
      const j = jangkauanKata(t, kata);
      return j ? [{ nilai, t, x0: j[0], x1: j[1] }] : [];
    }),
  );

  if (ditemukan.length === 0) return 'reguler';
  if (new Set(ditemukan.map((d) => d.nilai)).size === 1) return ditemukan[0].nilai;

  const bergaris = ditemukan.filter(({ t, x0, x1 }) =>
    h.kotak.some(
      (k) =>
        k.y1 - k.y0 <= 2 &&
        k.y0 >= t.y - 1 &&
        k.y0 <= t.y + t.ukuran * 0.4 &&
        Math.min(k.x1, x1) - Math.max(k.x0, x0) >= (x1 - x0) * 0.5,
    ),
  );

  if (bergaris.length !== 1) {
    throw new Error('Metode pengiriman (Instant/Sameday) tidak terbaca dari garis bawahnya.');
  }
  return bergaris[0].nilai;
}

function bacaTanggal(teks: string): string | null {
  const m = teks.match(/(\d{2})-(\d{2})-(\d{4})/);
  return m ? `${m[3]}-${m[2]}-${m[1]}` : null;
}

function bacaBarang(baris: Teks[][]): LabelShopee['items'] {
  const iKepala = baris.findIndex(
    (b) => b.some((t) => t.s.trim() === 'SKU') && b.some((t) => t.s.trim() === 'Qty'),
  );
  if (iKepala < 0) throw new Error('Tabel barang tidak ditemukan.');

  const kepala = baris[iKepala];
  const kolom = (pola: RegExp) => kepala.find((t) => pola.test(t.s.trim()));
  const nomor = kolom(/^#$/);
  const nama = kolom(/^Nama/);
  const sku = kolom(/^SKU$/);
  const variasi = kolom(/^Variasi$/);
  const qty = kolom(/^Qty$/);
  if (!nama || !sku || !variasi || !qty) throw new Error('Kolom tabel barang tidak lengkap.');

  const barang: { name: string[]; sku: string[]; variation: string[]; qty: string[] }[] = [];

  for (const b of baris.slice(iKepala + 1)) {
    const pertama = b[0];
    const nomorBaru =
      /^\d+$/.test(pertama.s.trim()) && Math.abs(pertama.x0 - (nomor?.x0 ?? pertama.x0)) < 4;

    // Baris di luar kolom nama yang bukan nomor urut (mis. "Pesan: ...") menandai akhir tabel.
    if (!nomorBaru && pertama.x0 < nama.x0 - 2) break;

    if (nomorBaru) barang.push({ name: [], sku: [], variation: [], qty: [] });
    const kini = barang.at(-1);
    if (!kini) break;

    for (const t of nomorBaru ? b.slice(1) : b) {
      if (t.x0 < sku.x0 - 2) kini.name.push(t.s);
      else if (t.x0 < variasi.x0 - 2) kini.sku.push(t.s.trim());
      else if (t.x0 < qty.x0 - 2) kini.variation.push(t.s);
      else kini.qty.push(t.s.trim());
    }
  }

  if (barang.length === 0) throw new Error('Label tidak memuat barang.');

  return barang.map((b) => {
    const jumlah = Number.parseInt(b.qty.join(''), 10);
    if (!Number.isFinite(jumlah)) throw new Error('Jumlah barang tidak terbaca.');
    const variation = rapi(b.variation.join(' '));
    return {
      name: rapi(b.name.join(' ')),
      // SKU yang terpotong ke baris berikutnya disambung tanpa spasi karena SKU tak pernah berspasi.
      sku: b.sku.join(''),
      variation: variation || null,
      qty: jumlah,
    };
  });
}

export function bacaLabel(h: Halaman): LabelShopee {
  const baris = kelompokkanBaris(h.teks);
  const tengah = h.lebar / 2;

  const noPesanan = cariTeks(h.teks, /No\.\s*Pesanan:/);
  const order_sn = noPesanan?.s.match(/No\.\s*Pesanan:\s*(\S+)/)?.[1] ?? '';
  if (!order_sn) throw new Error('Bukan label Shopee: nomor pesanan tidak ditemukan.');

  const penerima = cariTeks(h.teks, /^Penerima:/);
  if (!penerima) throw new Error(`Penerima pada pesanan ${order_sn} tidak ditemukan.`);
  const pengirim = cariTeks(h.teks, /^Pengirim:/);

  // Kolom kanan (data pengirim) dimulai di nilai "Pengirim:", bukan judulnya, karena alamat penerima boleh melewati judul itu.
  const barisPengirim = pengirim ? baris.find((b) => b.includes(pengirim)) : undefined;
  const nilaiPengirim = barisPengirim?.find((t) => pengirim && t.x0 >= pengirim.x1 - 1);
  const kolomKanan = nilaiPengirim?.x0 ?? pengirim?.x1 ?? h.lebar;

  const recipient_name = nilaiSetelah(baris, penerima, /^Penerima:/, pengirim?.x0);

  const berat = cariTeks(h.teks, /^Berat:/);
  if (!berat) throw new Error(`Kotak berat pada pesanan ${order_sn} tidak ditemukan.`);
  const iBerat = baris.findIndex((b) => b.includes(berat));
  const barisBayar = baris[iBerat - 1];
  const barisWilayah = baris[iBerat - 2];
  const iPenerima = baris.findIndex((b) => b.includes(penerima));
  if (!barisBayar || !barisWilayah || iBerat - 2 <= iPenerima) {
    throw new Error(`Kabupaten/kecamatan pada pesanan ${order_sn} tidak ditemukan.`);
  }

  const kiri = (b: Teks[]) => rapi(b.filter((t) => (t.x0 + t.x1) / 2 < tengah).map((t) => t.s).join(' '));
  const kanan = (b: Teks[]) => rapi(b.filter((t) => (t.x0 + t.x1) / 2 >= tengah).map((t) => t.s).join(' '));

  // Alamat penerima berbaris rata kiri dengan "Penerima:"; teks lain di sela (kode sortir, data pengirim) tidak.
  const address = rapi(
    baris
      .slice(iPenerima + 1, iBerat - 2)
      .filter((b) => Math.abs(b[0].x0 - penerima.x0) < 3)
      .flatMap((b) => b.filter((t) => t.x0 < kolomKanan - 1).map((t) => t.s))
      .join(' '),
  );

  const payment_label = kiri(barisBayar);
  const cod = cariTeks(h.teks, /^COD:/);
  const nilaiCod = cod ? nilaiSetelah(baris, cod, /^COD:/) : '';
  const is_cod = Number(nilaiCod.replace(/\D/g, '') || '0') > 0 || /\bCOD\b/.test(payment_label);

  const batas = cariTeks(h.teks, /^Batas Kirim:/);

  return {
    order_sn,
    recipient_name,
    address,
    city: kiri(barisWilayah),
    district: kanan(barisWilayah),
    payment_label,
    is_cod,
    shipping: bacaPengiriman(h),
    ship_by: batas ? bacaTanggal(nilaiSetelah(baris, batas, /^Batas Kirim:/)) : null,
    items: bacaBarang(baris),
  };
}
