/// <reference types="astro/client" />

import type { User } from './lib/api';

declare global {
  namespace App {
    interface Locals {
      user?: User;
      token?: string;
    }
  }
}

interface ImportMetaEnv {
  readonly BACKEND_URL: string;
}

export {};
