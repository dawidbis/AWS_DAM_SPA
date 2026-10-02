import { InjectionToken } from '@angular/core';

/**
 * Konfiguracja środowiska czytana przy starcie z `/config.json`.
 *
 * W AWS plik zapisuje Terraform (moduł frontend-hosting), lokalnie tworzy go
 * `just frontend-config`. Dzięki temu ten sam build działa w każdym środowisku.
 */
export interface RuntimeConfig {
  region: string;
  /** Issuer tokenów Cognito, np. https://cognito-idp.eu-central-1.amazonaws.com/<pool-id>. */
  authority: string;
  /** Adres managed login, np. https://<prefix>.auth.eu-central-1.amazoncognito.com. */
  authDomain: string;
  clientId: string;
  /** Bazowy URL API (od kolejnego kroku etapu 1). Tylko tam interceptor dokleja token. */
  apiUrl?: string;
}

export const RUNTIME_CONFIG = new InjectionToken<RuntimeConfig>('RUNTIME_CONFIG');

const REQUIRED_KEYS = ['region', 'authority', 'authDomain', 'clientId'] as const;

export async function loadRuntimeConfig(url = '/config.json'): Promise<RuntimeConfig> {
  const response = await fetch(url, { cache: 'no-store' });
  if (!response.ok) {
    throw new Error(`Nie udało się pobrać ${url} (HTTP ${response.status}).`);
  }
  const config = (await response.json()) as Partial<RuntimeConfig>;
  const missing = REQUIRED_KEYS.filter((key) => !config[key]);
  if (missing.length > 0) {
    throw new Error(`W ${url} brakuje pól: ${missing.join(', ')}.`);
  }
  return config as RuntimeConfig;
}
