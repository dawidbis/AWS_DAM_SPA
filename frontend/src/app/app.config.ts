import { ApplicationConfig, provideBrowserGlobalErrorListeners } from '@angular/core';
import { provideHttpClient, withInterceptors } from '@angular/common/http';
import { provideRouter, withComponentInputBinding } from '@angular/router';
import {
  authInterceptor,
  provideAuth,
  withAppInitializerAuthCheck,
} from 'angular-auth-oidc-client';

import { routes } from './app.routes';
import { buildOidcConfig } from './core/auth/oidc-config';
import { RUNTIME_CONFIG, RuntimeConfig } from './core/config/runtime-config';

export function createAppConfig(runtime: RuntimeConfig): ApplicationConfig {
  return {
    providers: [
      provideBrowserGlobalErrorListeners(),
      provideRouter(routes, withComponentInputBinding()),
      { provide: RUNTIME_CONFIG, useValue: runtime },
      provideHttpClient(withInterceptors([authInterceptor()])),
      provideAuth(
        { config: buildOidcConfig(runtime, window.location.origin) },
        withAppInitializerAuthCheck(),
      ),
    ],
  };
}
