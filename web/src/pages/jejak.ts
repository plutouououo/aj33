/** Penerus jejak klik dari skrip AppShell (`sendBeacon`) ke backend, karena browser tak pernah memanggil backend langsung; galat ditelan sebab jejak tak boleh mengganggu siapa pun. */
import type { APIRoute } from 'astro';
import { api } from '../lib/api';

export const POST: APIRoute = async ({ request, locals }) => {
  if (locals.token) {
    try {
      await api('/jejak', { method: 'POST', token: locals.token, body: await request.json() });
    } catch {
      // Jejak yang hilang hanya mengurangi sampel; tidak ada yang perlu diberi tahu.
    }
  }
  return new Response(null, { status: 204 });
};
