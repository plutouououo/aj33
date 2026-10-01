/** Klien HTTP ke backend Rust, hanya saat render di server; token dari cookie httpOnly diteruskan sebagai Bearer sehingga tak tersentuh JavaScript browser. */

/** Alamat backend dibaca saat request (`process.env`), bukan saat build, karena `import.meta.env` dibakukan Vite ke alamat mesin build; `import.meta.env` tetap cadangan untuk `astro dev`. */
const BACKEND_URL =
  (typeof process !== 'undefined' ? process.env.BACKEND_URL : undefined) ??
  import.meta.env.BACKEND_URL ??
  'http://localhost:3000';

/** Nama cookie sesi. Dipakai bersama `session.ts` dan middleware. */
export const COOKIE_SESI = 'aj33_sesi';

export interface ApiError {
  code: string;
  message: string;
}

/** Error backend dibawa apa adanya agar halaman menampilkan pesan yang sudah ditulis untuk pengguna, bukan pesan generik. */
export class ApiRequestError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
  ) {
    super();
  }

  static async dariResponse(res: Response): Promise<ApiRequestError> {
    let code = 'INTERNAL_ERROR';
    let message = 'Terjadi kesalahan pada server.';

    try {
      const body = (await res.json()) as { error?: ApiError };
      if (body.error) {
        code = body.error.code;
        message = body.error.message;
      }
    } catch {
      // Response bukan JSON (mis. backend mati, proxy menyisipkan HTML); pesan bawaan di atas sudah tepat.
    }

    const err = new ApiRequestError(res.status, code);
    err.message = message;
    return err;
  }
}

interface ApiOptions {
  token?: string;
  method?: 'GET' | 'POST' | 'PATCH' | 'DELETE';
  body?: unknown;
  headers?: Record<string, string>;
}

/** Byte mentah (unggahan impor) dikirim apa adanya tanpa `JSON.stringify` dan tanpa `Content-Type` otomatis; pemanggil menyetelnya lewat `headers`. */
function isBodyMentah(body: unknown): body is BodyInit {
  return (
    body instanceof ArrayBuffer || body instanceof Uint8Array || body instanceof Blob
  );
}

export async function api<T>(path: string, options: ApiOptions = {}): Promise<T> {
  const { token, method = 'GET', body, headers = {} } = options;
  const mentah = isBodyMentah(body);

  const res = await fetch(`${BACKEND_URL}/api${path}`, {
    method,
    headers: {
      ...(body !== undefined && !mentah ? { 'Content-Type': 'application/json' } : {}),
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
      ...headers,
    },
    body: body === undefined ? undefined : mentah ? body : JSON.stringify(body),
  });

  if (!res.ok) {
    throw await ApiRequestError.dariResponse(res);
  }

  if (res.status === 204) {
    return undefined as T;
  }

  return (await res.json()) as T;
}

// --- Bentuk data dari backend ---

export type Role = 'owner' | 'kasir' | 'pengepak';

export interface User {
  id: string;
  name: string;
  email_or_username: string;
  role: Role;
  phone: string | null;
  is_active: boolean;
  /** Password dipasang orang lain dan belum diganti pemiliknya; selama true middleware menahan pengguna di `/pengaturan/akun` (lihat `bolehAkses`). */
  must_change_password: boolean;
}

export interface Product {
  id: string;
  category_id: string | null;
  category_name: string | null;
  /** Nama identifikasi internal: yang dicari pegawai dan dibaca pengepak. */
  name: string;
  /** Judul untuk marketplace. `null` berarti belum diisi. */
  seo_name: string | null;
  /** SKU dirakit otomatis sekali saat produk dibuat lalu dibeku (`Jenis+Grade - Merek - Ukuran`); varian menambah sumbunya di belakang SKU induk sehingga satu produk berbagi satu awalan. */
  sku: string | null;
  brand_name: string | null;
  product_type: string | null;
  /** Mutu / kelas ukuran, mis. "SP 08", "Super Besar". */
  variant_grade: string | null;
  /** Isi satu pack dalam kg (boleh pecahan, mis. 0,9). `null` berarti kasir selalu memakai harga ecer. Satu SKU berarti satu pack. */
  variant_size: number | null;
  /** Terisi berarti produk ini varian dari produk lain. */
  parent_id: string | null;
  /** Induk yang punya varian tidak dijual langsung — variannya yang dijual. */
  variant_count: number;
  /** Harga ecer per pack: yang dipakai kasir bila berat baris tak melebihi batas grosir, sekaligus rujukan saat harga kanal kosong. */
  price: number;
  /** Harga grosir per pack, hanya kanal toko. `null` berarti belum diatur -- kasir tetap memakai harga ecer. */
  price_wholesale: number | null;
  /** `null` berarti belum diatur -- bukan gratis. */
  price_shopee: number | null;
  /** Mencakup Tokopedia; satu kanal dengan TikTok Shop. */
  price_tiktok: number | null;
  cost_price: number | null;
  stock_qty: number;
  low_stock_threshold: number;
  image_url: string | null;
  /** Kedaluwarsa terdekat dari seluruh batch produk ini. */
  nearest_expiry: string | null;
  is_active: boolean;
  created_at: string;
  updated_at: string;
}

/** Satu kiriman masuk: `quantity` isi saat datang (tetap), `remaining_qty` sisa belum keluar adalah stok sungguhan (sejak migrasi 0011 = `Product.stock_qty`). */
export interface ProductBatch {
  id: string;
  product_id: string;
  batch_number: string | null;
  /** Harga beli per batch, dipakai untuk menghitung laba. */
  purchase_price: number | null;
  /** Isi kiriman saat datang. Tidak pernah berubah. */
  quantity: number;
  /** Sisa yang belum keluar. Inilah yang dikurangi penjualan. */
  remaining_qty: number;
  expiry_date: string | null;
  /** Rak tempat kiriman ini ditaruh; milik batch, bukan produk, karena dua kiriman produk yang sama bisa di rak berbeda (migrasi 0016). */
  storage_location: string | null;
  received_at: string;
}

/** Bentuk `GET /products/{id}`: produk beserta varian dan batch-nya. */
export interface ProductDetail extends Product {
  variants: Product[];
  batches: ProductBatch[];
}

export interface PaginatedProducts {
  data: Product[];
  page: number;
  limit: number;
  total: number;
}

export interface Category {
  id: string;
  name: string;
}

/** Batas harga grosir: baris kanal toko dihargai grosir bila berat baris (jumlah × ukuran pack) LEBIH dari angka ini. */
export interface PricingSettings {
  wholesale_threshold_kg: number;
}

/** Bagian atribut yang punya kode sendiri di kamus SKU. */
export type SkuKind = 'jenis' | 'grade' | 'merek' | 'ukuran';

/** Entri kamus kode SKU memetakan atribut ke kode pendek dan menang atas inisial bebas; entri baru tak mengubah SKU produk yang sudah ada (beku sejak dirakit). */
export interface SkuCode {
  id: string;
  kind: SkuKind;
  /** Nilai atribut apa adanya, mis. "SP 08". */
  source: string;
  code: string;
}

export interface TransactionItem {
  id: string;
  product_id: string;
  product_name_snapshot: string;
  qty: number;
  unit_price: number;
  subtotal: number;
}

/** Daftar harga saat menjual; Shopee/Tokopedia dicatat manual di kasir dengan harga kanalnya, dan `tiktok` mencakup Tokopedia (satu kanal sejak akuisisi TikTok). */
export type SalesChannel = 'toko' | 'shopee' | 'tiktok';

export interface Transaction {
  id: string;
  type: string;
  payment_method: string;
  sales_channel: SalesChannel;
  subtotal: number;
  /** Diskon umum lama: selalu 0 untuk transaksi baru, hanya transaksi lama yang memuatnya. */
  discount_amount: number;
  /** Ongkir sebenarnya, tidak termasuk `subtotal`; menambah `total_amount` hanya bila pembeli yang menanggung. */
  shipping_cost: number;
  /** Diskon ongkir: toko menanggung ongkir (beban toko) dan pembeli tidak membayarnya. */
  shipping_borne_by_store: boolean;
  /** Untuk Shopee ini bukan yang dibayar pembeli melainkan uang yang cair ke toko setelah empat biaya platform dipotong; kanal lain tetap "yang dibayar pembeli". */
  total_amount: number;
  /** Nama field mengikuti `v2.payment.get_escrow_detail` Shopee (lihat `marketplace/shopee/client.rs` dan `pos::service`). */
  /** Persentase `commission_fee` transaksi ini (pecahan, 0,1725 = 17,25%), nol selain Shopee, bisa diedit kasir per transaksi. */
  platform_commission_fee_percent: number;
  platform_commission_fee: number;
  /** Persentase `service_fee` (program opsional seperti Gratis Ongkir Xtra/Star+), nol bila tak ikut, bisa diedit kasir. */
  platform_service_fee_percent: number;
  platform_service_fee: number;
  /** `withholding_tax`: PPh final UMKM 0,5% dari omzet Shopee setelah diskon, tetap. */
  platform_withholding_tax: number;
  /** `seller_order_processing_fee`: Rp1.250 tetap sekali per transaksi Shopee. */
  platform_order_processing_fee: number;
  amount_paid: number | null;
  change_amount: number | null;
  status: string;
  created_at: string;
  items: TransactionItem[];
}

/** Pelanggan toko; `name` boleh null (hasil impor marketplace kadang hanya username), angka belanja selalu ikut agar tak ada dua daftar yang menyimpang, `total_spent` tanpa ongkir. */
export interface Customer {
  id: string;
  name: string | null;
  phone: string | null;
  /** Alamat antar. `null` untuk pembeli yang datang ke toko. */
  address: string | null;
  /** `walk_in` untuk yang dibuat di kasir, `marketplace` untuk hasil impor. */
  source: string;
  created_at: string;
  purchase_count: number;
  total_spent: number;
  /** `null` kalau belum pernah belanja. */
  last_purchase_at: string | null;
}

export interface PaginatedCustomers {
  data: Customer[];
  page: number;
  limit: number;
  total: number;
}

export interface FavoriteProduct {
  /** `null` kalau produknya sudah dihapus. */
  product_id: string | null;
  name: string;
  qty: number;
  spent: number;
}

/** Satu baris riwayat belanja seorang pelanggan. */
export interface PurchaseRow {
  id: string;
  created_at: string;
  payment_method: string;
  item_count: number;
  revenue: number;
  shipping: number;
  total_amount: number;
}

/** Bentuk `GET /customers/{id}`: pelanggan beserta riwayat dan favoritnya. */
export interface CustomerDetail extends Customer {
  favorite_products: FavoriteProduct[];
  purchases: PurchaseRow[];
}

export interface StockAdjustment {
  id: string;
  product_id: string;
  change_qty: number;
  reason: string;
  reference_type: string | null;
  reference_id: string | null;
  stock_before: number;
  stock_after: number;
  created_at: string;
}

export type TicketStatus = 'unassigned' | 'assigned' | 'packing' | 'packed' | 'handed_over';

export interface TicketItem {
  id: string;
  product_id: string;
  product_name_snapshot: string;
  qty: number;
  is_packed: boolean;
  /** Rak tempat barang masih ada, urut FEFO dipisah koma, dibaca dari batch bersisa sehingga selalu keadaan sekarang. */
  storage_location: string | null;
}

export interface Ticket {
  id: string;
  external_order_id: string;
  order_ref: string;
  platform_name: string;
  sla_type: string;
  sla_deadline: string | null;
  status: TicketStatus;
  assigned_to_user_id: string | null;
  assigned_to_name: string | null;
  assigned_at: string | null;
  completed_at: string | null;
  notes: string | null;
  items: TicketItem[];
}

// --- Dasbor dan laporan ---

/** Satu baris penjualan: `revenue` = omzet barang setelah diskon tanpa `shipping`, sedangkan `total_amount` yang dibayar pembeli; memakai `total_amount` sebagai omzet membuat margin salah. */
export interface SaleRow {
  id: string;
  created_at: string;
  /** `null` untuk pembeli yang namanya tidak dicatat kasir. */
  customer_name: string | null;
  type: string;
  payment_method: string;
  item_count: number;
  revenue: number;
  shipping: number;
  total_amount: number;
}

export interface LowStockProduct {
  id: string;
  name: string;
  sku: string | null;
  stock_qty: number;
  low_stock_threshold: number;
}

export interface Dashboard {
  today_revenue: number;
  month_revenue: number;
  /** Omzet kemarin, untuk pil naik/turun di kartu "Omzet Hari Ini". */
  yesterday_revenue: number;
  /** Omzet bulan lalu, untuk pil naik/turun di kartu "Omzet Bulan Ini". */
  last_month_revenue: number;
  today_transaction_count: number;
  product_count: number;
  customer_count: number;
  low_stock_count: number;
  /** Potret persediaan sekarang, bukan angka periode. */
  stock_value: StockValue;
  recent_sales: SaleRow[];
  low_stock_products: LowStockProduct[];
}

export interface SalesSummary {
  revenue: number;
  /** Ongkir yang ditagih ke pembeli; ongkir yang ditanggung toko ada di `shipping_subsidy`. */
  shipping: number;
  /** Ongkir ditanggung toko lewat diskon ongkir; sudah termasuk di `expenses`. */
  shipping_subsidy: number;
  /** Berat barang terjual dalam kg (pack × ukuran); penjualan produk tanpa ukuran tak ikut. */
  sold_kg: number;
  cogs: number;
  /** Beban toko, termasuk `shipping_subsidy`. */
  expenses: number;
  /** Jumlah biaya admin + PPh 0,5% + biaya proses Rp1.250 Shopee periode ini, dibaca dari yang tersimpan per transaksi (bukan tarif tetap); nol bila tak ada penjualan Shopee. */
  platform_fees: number;
  /** `revenue - cogs - expenses - platform_fees`. */
  profit: number;
  transaction_count: number;
  /** Baris item yang harga pokoknya belum diisi; selama bukan nol, `cogs` dan `profit` hanyalah batas atas dan halaman mengatakannya. */
  items_without_cost: number;
}

export interface MonthlyPoint {
  /** `YYYY-MM` pada zona toko. */
  month: string;
  revenue: number;
  expenses: number;
}

export interface ExpenseSlice {
  category: string;
  amount: number;
}

export interface TopProduct {
  /** `null` kalau produknya sudah dihapus. */
  product_id: string | null;
  name: string;
  sku: string | null;
  qty: number;
  revenue: number;
}

export interface StockValue {
  /** Σ sisa stok × harga beli batch (cadangan: harga modal produk). */
  value: number;
  /** Batch bersisa tanpa harga beli; selama bukan nol, `value` adalah batas bawah. */
  batches_without_cost: number;
  /** Total berat stok dalam kg (sisa pack × ukuran); pack tanpa ukuran tak ikut, jadi batas bawah selama `packs_without_size` bukan nol. */
  total_kg: number;
  /** Pack bersisa yang produknya belum punya ukuran. */
  packs_without_size: number;
}

export interface SalesReport {
  summary: SalesSummary;
  /** Angka periode setara sebelumnya untuk kartu trend; `null` bila saringan "Seluruh Waktu" karena tak punya pembanding. */
  previous_summary: SalesSummary | null;
  /** Selalu enam bulan terakhir, tidak ikut saringan periode. */
  trend: MonthlyPoint[];
  expense_breakdown: ExpenseSlice[];
  top_products: TopProduct[];
  /** Seluruh produk terlaris pada periodenya, bukan hanya halaman ini. */
  top_products_total: number;
  sales: SaleRow[];
  sales_limit: number;
}

/** Satu baris item pada detail transaksi. */
export interface TransactionDetailItem {
  id: string;
  /** `null` kalau produknya sudah dihapus. */
  product_id: string | null;
  /** Nama saat terjual, bukan nama sekarang. */
  name: string;
  /** SKU produk saat ini. `null` kalau produknya sudah dihapus. */
  sku: string | null;
  qty: number;
  unit_price: number;
  subtotal: number;
}

/** Bentuk `GET /reports/sales/{id}`. Owner saja. */
export interface TransactionDetail {
  id: string;
  created_at: string;
  status: string;
  voided_at: string | null;
  void_reason: string | null;
  type: string;
  sales_channel: SalesChannel;
  payment_method: string;
  customer_name: string | null;
  customer_phone: string | null;
  customer_email: string | null;
  cashier_name: string;
  subtotal: number;
  discount_amount: number;
  shipping_cost: number;
  shipping_borne_by_store: boolean;
  total_amount: number;
  amount_paid: number | null;
  change_amount: number | null;
  platform_commission_fee_percent: number;
  platform_commission_fee: number;
  platform_service_fee_percent: number;
  platform_service_fee: number;
  platform_withholding_tax: number;
  platform_order_processing_fee: number;
  /** Batas atas kalau `items_without_cost > 0`. */
  cogs: number;
  items_without_cost: number;
  /** `subtotal - discount_amount - cogs` dikurangi seluruh potongan Shopee dan ongkir yang ditanggung toko; beban toko lain tak ikut karena milik periode, bukan transaksi. */
  net_profit: number;
  items: TransactionDetailItem[];
}

// --- Impor produk massal ---

export type ImportStatus =
  | 'draft'
  | 'pending_review'
  | 'approved'
  | 'committing'
  | 'committed'
  | 'cancelled';

export type ImportRowAction = 'create' | 'update' | 'skip';
export type ImportCommitState = 'pending' | 'ok' | 'failed';

export interface ImportIssue {
  level: 'error' | 'warn';
  field: string;
  msg: string;
}

export interface ImportBatch {
  id: string;
  file_name: string;
  status: ImportStatus;
  total_rows: number;
  publish_on_commit: boolean;
  uploaded_at: string;
  submitted_at: string | null;
  reviewed_at: string | null;
  review_note: string | null;
  committed_at: string | null;
  ok_count: number;
  fail_count: number;
}

/** Bentuk `GET /imports/{id}`: batch beserta ringkasan barisnya. */
export interface ImportBatchDetail extends ImportBatch {
  create_count: number;
  update_count: number;
  skip_count: number;
  error_count: number;
  warn_count: number;
}

export interface PaginatedImportBatches {
  data: ImportBatch[];
  page: number;
  limit: number;
  total: number;
}

export interface ImportRow {
  id: string;
  row_no: number;
  name: string | null;
  sku: string | null;
  category_text: string | null;
  brand_text: string | null;
  product_type: string | null;
  variant_grade: string | null;
  variant_size: string | null;
  category_id: string | null;
  lowest_price: number | null;
  cost: number | null;
  margin_pct: number | null;
  stock: number | null;
  published: boolean | null;
  action: ImportRowAction;
  match_product_id: string | null;
  issues: ImportIssue[];
  commit_state: ImportCommitState;
  commit_product_id: string | null;
  commit_error: string | null;
}

export interface PaginatedImportRows {
  data: ImportRow[];
  page: number;
  limit: number;
  total: number;
}
