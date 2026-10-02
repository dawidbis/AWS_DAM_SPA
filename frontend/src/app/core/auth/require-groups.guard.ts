import { inject } from '@angular/core';
import { CanActivateFn, Router } from '@angular/router';

import { AuthService } from './auth.service';
import { UserGroup } from './user-group';

/**
 * Wpuszcza na trasę tylko zalogowanych członków podanych grup.
 * Niezalogowanych kieruje do logowania, pozostałych na /forbidden.
 * To ochrona UX, nie bezpieczeństwa: API sprawdza grupy w tokenie samo.
 */
export function requireGroups(...allowed: UserGroup[]): CanActivateFn {
  return (_route, state) => {
    const auth = inject(AuthService);
    const router = inject(Router);

    if (!auth.isAuthenticated()) {
      auth.login(state.url);
      return false;
    }
    return auth.hasAnyGroup(allowed) ? true : router.parseUrl('/forbidden');
  };
}
