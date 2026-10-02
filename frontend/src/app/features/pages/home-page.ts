import { Component, effect, inject, signal } from '@angular/core';

import { MeResponse, MeService } from '../../core/api/me.service';
import { AuthService } from '../../core/auth/auth.service';
import { GROUP_LABELS } from '../../core/auth/user-group';

type BackendCheck = { state: 'idle' | 'loading' | 'error' } | { state: 'ok'; me: MeResponse };

@Component({
  selector: 'app-home-page',
  template: `
    <div class="hero bg-base-200 rounded-box min-h-64">
      <div class="hero-content text-center">
        <div class="max-w-xl">
          <h1 class="text-4xl font-bold">Matchday DAM</h1>
          <p class="py-4">Bezpieczne repozytorium materiałów medialnych klubu KS Matchday.</p>
          @if (auth.isAuthenticated()) {
            <p>
              Zalogowano jako <strong>{{ auth.email() }}</strong>
            </p>
            <div class="mt-2 flex flex-wrap justify-center gap-2">
              @for (group of auth.groups(); track group) {
                <span class="badge badge-primary">{{ labels[group] }}</span>
              } @empty {
                <span class="badge badge-warning">Brak przypisanej grupy</span>
              }
            </div>

            @let check = backend();
            <p class="mt-4 text-sm opacity-80" data-testid="backend-check">
              @switch (check.state) {
                @case ('loading') {
                  Sprawdzanie tokenu w API…
                }
                @case ('ok') {
                  @if (check.state === 'ok') {
                    API potwierdza grupy:
                    {{ check.me.groups.length ? check.me.groups.join(', ') : 'brak' }}
                  }
                }
                @case ('error') {
                  <span class="text-error">API odrzuciło żądanie lub jest niedostępne.</span>
                }
              }
            </p>
          } @else {
            <button class="btn btn-primary" type="button" (click)="auth.login('/gallery')">
              Zaloguj się
            </button>
          }
        </div>
      </div>
    </div>
  `,
})
export class HomePage {
  protected readonly auth = inject(AuthService);
  protected readonly labels = GROUP_LABELS;
  private readonly me = inject(MeService);

  protected readonly backend = signal<BackendCheck>({ state: 'idle' });

  constructor() {
    // Po zalogowaniu sprawdzamy, czy backend akceptuje token i widzi te same grupy.
    effect(() => {
      if (!this.auth.isAuthenticated() || !this.me.available) {
        return;
      }
      this.backend.set({ state: 'loading' });
      this.me.get().subscribe({
        next: (me) => this.backend.set({ state: 'ok', me }),
        error: () => this.backend.set({ state: 'error' }),
      });
    });
  }
}
