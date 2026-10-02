import { signal } from '@angular/core';
import { OidcSecurityService } from 'angular-auth-oidc-client';

import { RUNTIME_CONFIG, RuntimeConfig } from '../core/config/runtime-config';

export const TEST_RUNTIME_CONFIG: RuntimeConfig = {
  region: 'eu-central-1',
  authority: 'https://cognito-idp.eu-central-1.amazonaws.com/eu-central-1_TEST',
  authDomain: 'https://matchday-test.auth.eu-central-1.amazoncognito.com',
  clientId: 'test-client',
};

/** Atrapa OidcSecurityService sterowana sygnałami (tylko to, czego używa aplikacja). */
export class FakeOidcSecurityService {
  readonly authenticated = signal({ isAuthenticated: false, allConfigsAuthenticated: [] });
  readonly userData = signal<{ userData: Record<string, unknown> | null; allUserData: [] }>({
    userData: null,
    allUserData: [],
  });
  readonly authorize = vi.fn();
  readonly logoffLocal = vi.fn();

  signIn(claims: Record<string, unknown>): void {
    this.authenticated.set({ isAuthenticated: true, allConfigsAuthenticated: [] });
    this.userData.set({ userData: claims, allUserData: [] });
  }
}

export function provideFakeAuth(fake = new FakeOidcSecurityService()) {
  return [
    { provide: OidcSecurityService, useValue: fake },
    { provide: RUNTIME_CONFIG, useValue: TEST_RUNTIME_CONFIG },
  ];
}
