/**
 * Klien HTTP ke backend Rust.
 *
 * Hanya dipakai saat Astro merender di server. Token sesi diambil dari
 * cookie httpOnly dan diteruskan sebagai `Authorization: Bearer`, jadi token
 * tidak pernah tersentuh JavaScript di browser.
 */

/**
 * Alamat backend, dibaca saat request berjalan -- bukan saat build.
 *
 * `import.meta.env.X` diganti Vite dengan nilai literalnya ketika di-build,
 * jadi kalau dipakai sendirian, alamat yang ikut ke dist adalah alamat mesin
 * yang mem-build (biasanya `http://localhost:3000`) dan menyetel BACKEND_URL
 * di server produksi tidak berpengaruh apa-apa. Adapter Node menjalankan
 * halaman ini di Node sungguhan, jadi `process.env` tersedia dan itulah yang
 * dibaca lebih dulu; `import.meta.env` tetap dipakai sebagai cadangan supaya
 * `.env` saat `astro dev` tetap bekerja.
 */
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

/**
 * Error dari backend dibawa apa adanya supaya halaman bisa menampilkan
 * pesan yang sudah ditulis untuk pengguna, bukan pesan generik buatan
 * frontend yang kehilangan konteksnya.
 */
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
      // Response bukan JSON (mis. backend mati, proxy menyisipkan HTML).
      // Pesan bawaan di atas sudah tepat.
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

export async function api<T>(path: string, options: ApiOptions = {}): Promise<T> {
  const { token, method = 'GET', body, headers = {} } = options;

  const res = await fetch(`${BACKEND_URL}/api${path}`, {
    method,
    headers: {
      ...(body !== undefined ? { 'Content-Type': 'application/json' } : {}),
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
      ...headers,
    },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });

  if (!res.ok) {
    throw await ApiRequestError.dariResponse(res);
  }

  if (res.status === 204) {
    return undefined as T;
  }

  return (await res.json()) as T;
}

// ---------------------------------------------------------------------
// Bentuk data dari backend
// ---------------------------------------------------------------------

export type Role = 'owner' | 'kasir' | 'pengepak';

export interface User {
  id: string;
  name: string;
  email_or_username: string;
  role: Role;
  phone: string | null;
  is_active: boolean;
  /**
   * Password akun ini dipasang orang lain dan belum pernah diganti
   * pemiliknya. Selama true, middleware menahan pengguna di
   * `/ganti-password` -- lihat `bolehAkses` di `session.ts`.
   */
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
  /**
   * Dirakit otomatis sekali saat produk dibuat, lalu dibekukan: menyunting
   * atribut tidak mengubahnya. Induk berbentuk `Jenis+Grade - Merek -
   * Ukuran`; varian menambahkan sumbu variannya di belakang SKU induknya,
   * jadi seluruh varian satu produk berbagi satu awalan.
   */
  sku: string | null;
  brand_name: string | null;
  product_type: string | null;
  /** Mutu / kelas ukuran, mis. "SP 08", "Super Besar". */
  variant_grade: string | null;
  /** Isi satu pack, mis. "2 kg". Satu SKU berarti satu pack. */
  variant_size: string | null;
  /** Terisi berarti produk ini varian dari produk lain. */
  parent_id: string | null;
  /** Induk yang punya varian tidak dijual langsung — variannya yang dijual. */
  variant_count: number;
  /** Harga dasar: yang dipakai kasir, sekaligus rujukan saat harga kanal kosong. */
  price: number;
  /** `null` berarti belum diatur -- bukan gratis. */
  price_shopee: number | null;
  /** Mencakup Tokopedia; satu kanal dengan TikTok Shop. */
  price_tiktok: number | null;
  cost_price: number | null;
  stock_qty: number;
  low_stock_threshold: number;
  image_url: string | null;
  /** Label rak internal, mis. "Rak A3". */
  storage_location: string | null;
  /** Kedaluwarsa terdekat dari seluruh batch produk ini. */
  nearest_expiry: string | null;
  is_active: boolean;
  created_at: string;
  updated_at: string;
}

/**
 * Satu kiriman barang masuk.
 *
 * `quantity` adalah isi kiriman saat datang dan tidak pernah berubah;
 * `remaining_qty` adalah sisa yang belum keluar, dan itulah stok sungguhan.
 * Sejak migrasi 0011, `Product.stock_qty` sama dengan jumlah `remaining_qty`
 * seluruh batch produk itu.
 */
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

/** Bagian atribut yang punya kode sendiri di kamus SKU. */
export type SkuKind = 'jenis' | 'grade' | 'merek' | 'ukuran';

/**
 * Satu entri kamus kode SKU: pemetaan nilai atribut ke kode pendek.
 *
 * Kode dari kamus menang atas inisial bebas. Entri baru TIDAK mengubah SKU
 * produk yang sudah ada -- SKU beku sejak dirakit -- hanya produk yang dibuat
 * sesudahnya.
 */
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

export interface Transaction {
  id: string;
  type: string;
  payment_method: string;
  subtotal: number;
  /** Ongkir tidak termasuk `subtotal`; hanya menambah `total_amount`. */
  shipping_cost: number;
  total_amount: number;
  amount_paid: number | null;
  change_amount: number | null;
  status: string;
  created_at: string;
  items: TransactionItem[];
}

/**
 * Pelanggan toko. `name` boleh null sejak skema awal — pelanggan hasil impor
 * pesanan marketplace kadang hanya membawa username.
 *
 * Angka belanja selalu ikut, termasuk saat kasir hanya butuh daftar nama:
 * satu bentuk untuk satu hal, supaya tidak ada dua daftar pelanggan yang
 * bisa saling menyimpang. `total_spent` adalah harga barang, tanpa ongkir.
 */
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
  product_id: string;
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
  /** Dibaca langsung dari produk, jadi selalu rak yang berlaku sekarang. */
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

// ---------------------------------------------------------------------
// Dasbor dan laporan
// ---------------------------------------------------------------------

/**
 * Satu baris penjualan pada dasbor maupun laporan.
 *
 * `revenue` adalah omzet barang (`subtotal` di backend) dan tidak termasuk
 * `shipping`; `total_amount` adalah jumlah yang benar-benar dibayar pembeli.
 * Memakai `total_amount` sebagai omzet membuat margin salah -- ongkir tidak
 * punya margin.
 */
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
  today_transaction_count: number;
  product_count: number;
  customer_count: number;
  low_stock_count: number;
  recent_sales: SaleRow[];
  low_stock_products: LowStockProduct[];
}

export interface SalesSummary {
  revenue: number;
  shipping: number;
  cogs: number;
  expenses: number;
  profit: number;
  transaction_count: number;
  /**
   * Baris item yang harga pokoknya belum diisi. Selama bukan nol, `cogs`
   * dan `profit` adalah batas atas -- dan halaman mengatakannya.
   */
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
  product_id: string;
  name: string;
  sku: string | null;
  qty: number;
  revenue: number;
}

export interface SalesReport {
  summary: SalesSummary;
  /** Selalu enam bulan terakhir, tidak ikut saringan periode. */
  trend: MonthlyPoint[];
  expense_breakdown: ExpenseSlice[];
  top_products: TopProduct[];
  sales: SaleRow[];
  sales_limit: number;
}
