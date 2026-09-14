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
}

export interface Product {
  id: string;
  category_id: string | null;
  category_name: string | null;
  /** Nama identifikasi internal: yang dicari pegawai dan dibaca pengepak. */
  name: string;
  /** Judul untuk marketplace. `null` berarti belum diisi. */
  seo_name: string | null;
  /** Selalu hasil rakitan [Merek] - [Jenis] - [Warna] - [Ukuran]. */
  sku: string | null;
  brand_name: string | null;
  product_type: string | null;
  variant_color: string | null;
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
  unit: string | null;
  /** Label rak internal, mis. "Rak A3". */
  storage_location: string | null;
  /** Kedaluwarsa terdekat dari seluruh batch produk ini. */
  nearest_expiry: string | null;
  is_active: boolean;
  created_at: string;
  updated_at: string;
}

/**
 * Satu catatan barang masuk. `quantity` adalah jumlah yang masuk saat itu,
 * bukan sisa yang belum terjual — stok berjalan tetap di `Product.stock_qty`.
 */
export interface ProductBatch {
  id: string;
  product_id: string;
  batch_number: string | null;
  quantity: number;
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
 */
export interface Customer {
  id: string;
  name: string | null;
  phone: string | null;
  /** `walk_in` untuk yang dibuat di kasir, `marketplace` untuk hasil impor. */
  source: string;
  created_at: string;
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
