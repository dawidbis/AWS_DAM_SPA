import { ApplicationConfig, provideBrowserGlobalErrorListeners } from '@angular/core';
import { provideHttpClient, withInterceptors } from '@angular/common/http';
import { provideRouter, withComponentInputBinding } from '@angular/router';
import {
  LogLevel,
  authInterceptor,
  provideAuth,
  withAppInitializerAuthCheck,
} from 'angular-auth-oidc-client';

import { routes } from './app.routes';
import { RUNTIME_CONFIG, RuntimeConfig } from './core/config/runtime-config';

export function createAppConfig(runtime: RuntimeConfig): ApplicationConfig {
  const origin = window.location.origin;

  return {
    providers: [
      provideBrowserGlobalErrorListeners(),
      provideRouter(routes, withComponentInputBinding()),
      { provide: RUNTIME_CONFIG, useValue: runtime },
      // Token trafia wyłącznie do naszego API (secureRoutes), nigdy do innych hostów.
      provideHttpClient(withInterceptors([authInterceptor()])),
      provideAuth(
        {
          config: {
            authority: runtime.authority,
            clientId: runtime.clientId,
            redirectUrl: `${origin}/auth/callback`,
            postLogoutRedirectUri: `${origin}/`,
            scope: 'openid email profile',
            responseType: 'code',
            silentRenew: true,
            useRefreshToken: true,
            // Cognito nie rotuje refresh tokenów i nie zwraca nonce po odświeżeniu.
            allowUnsafeReuseRefreshToken: true,
            ignoreNonceAfterRefresh: true,
            renewTimeBeforeTokenExpiresInSeconds: 60,
            secureRoutes: runtime.apiUrl ? [runtime.apiUrl] : [],
            logLevel: LogLevel.Warn,
          },
        },
        withAppInitializerAuthCheck(),
      ),
    ],
  };
}
