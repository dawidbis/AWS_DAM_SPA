import { Component, inject } from '@angular/core';

import { AuthService } from '../../core/auth/auth.service';
import { GROUP_LABELS } from '../../core/auth/user-group';

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
}
