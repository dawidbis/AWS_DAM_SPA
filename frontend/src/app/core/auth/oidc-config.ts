import { LogLevel, OpenIdConfiguration } from 'angular-auth-oidc-client';

import { RuntimeConfig } from '../config/runtime-config';

/** Konfiguracja klienta OIDC dla Cognito (authorization code + PKCE). */
export function buildOidcConfig(runtime: RuntimeConfig, origin: string): OpenIdConfiguration {
  return {
    authority: runtime.authority,
    clientId: runtime.clientId,
    redirectUrl: `${origin}/auth/callback`,
    postLogoutRedirectUri: `${origin}/`,
    scope: 'openid email profile',
    responseType: 'code',
    // Dane użytkownika bierzemy z ID tokenu, a nie z endpointu userInfo:
    // tylko token zawiera claim `cognito:groups`. Po odświeżeniu tokenów
    // dane są aktualizowane, więc zmiana grupy dociera bez ponownego logowania.
    autoUserInfo: false,
    renewUserInfoAfterTokenRenew: true,
    silentRenew: true,
    useRefreshToken: true,
    // Cognito nie rotuje refresh tokenów i nie zwraca nonce po odświeżeniu.
    allowUnsafeReuseRefreshToken: true,
    ignoreNonceAfterRefresh: true,
    renewTimeBeforeTokenExpiresInSeconds: 60,
    // Token trafia wyłącznie do naszego API, nigdy do innych hostów.
    secureRoutes: runtime.apiUrl ? [runtime.apiUrl] : [],
    logLevel: LogLevel.Warn,
  };
}
