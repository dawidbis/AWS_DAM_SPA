import { Component, OnInit, inject } from '@angular/core';
import { Router } from '@angular/router';

import { AuthService } from './auth.service';

/**
 * Cel przekierowania z Cognito. Wymianę kodu na tokeny wykonuje
 * `withAppInitializerAuthCheck()` przed startem routera, tu tylko wracamy
 * na stronę, z której użytkownik zaczął logowanie.
 */
@Component({
  selector: 'app-auth-callback',
  template: `<p class="p-6 text-center">Logowanie…</p>`,
})
export class AuthCallback implements OnInit {
  private readonly auth = inject(AuthService);
  private readonly router = inject(Router);

  ngOnInit(): void {
    const target = this.auth.isAuthenticated() ? this.auth.consumeReturnUrl() : '/';
    void this.router.navigateByUrl(target, { replaceUrl: true });
  }
}
