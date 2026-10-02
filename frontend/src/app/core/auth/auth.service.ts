import { Injectable, computed, inject } from '@angular/core';
import { OidcSecurityService } from 'angular-auth-oidc-client';

import { RUNTIME_CONFIG } from '../config/runtime-config';
import { UserGroup, isUserGroup } from './user-group';

const RETURN_URL_KEY = 'matchday.returnUrl';

/**
 * Stan zalogowanego użytkownika w formie sygnałów.
 *
 * Grupy pochodzą z claimu `cognito:groups` tokenu. Ukrywanie elementów UI na
 * ich podstawie to wyłącznie UX: uprawnienia egzekwuje backend.
 */
@Injectable({ providedIn: 'root' })
export class AuthService {
  private readonly oidc = inject(OidcSecurityService);
  private readonly config = inject(RUNTIME_CONFIG);

  readonly isAuthenticated = computed(() => this.oidc.authenticated().isAuthenticated);

  private readonly claims = computed<Record<string, unknown>>(
    () => this.oidc.userData().userData ?? {},
  );

  readonly email = computed(() => {
    const email = this.claims()['email'];
    return typeof email === 'string' ? email : null;
  });

  readonly groups = computed<UserGroup[]>(() => {
    const groups = this.claims()['cognito:groups'];
    return Array.isArray(groups) ? groups.filter(isUserGroup) : [];
  });

  hasAnyGroup(allowed: readonly UserGroup[]): boolean {
    const groups = this.groups();
    return allowed.some((group) => groups.includes(group));
  }

  /** Przekierowuje do managed login Cognito (authorization code + PKCE). */
  login(returnUrl = '/'): void {
    sessionStorage.setItem(RETURN_URL_KEY, returnUrl);
    this.oidc.authorize();
  }

  /** Zwraca i czyści adres zapamiętany przed logowaniem (tylko ścieżki lokalne). */
  consumeReturnUrl(): string {
    const url = sessionStorage.getItem(RETURN_URL_KEY);
    sessionStorage.removeItem(RETURN_URL_KEY);
    return url?.startsWith('/') && !url.startsWith('//') ? url : '/';
  }

  /**
   * Czyści sesję lokalnie i w Cognito. Discovery document Cognito nie zawiera
   * end_session_endpoint, dlatego adres /logout składamy sami.
   */
  logout(): void {
    this.oidc.logoffLocal();
    const params = new URLSearchParams({
      client_id: this.config.clientId,
      logout_uri: `${window.location.origin}/`,
    });
    window.location.assign(`${this.config.authDomain}/logout?${params.toString()}`);
  }
}
