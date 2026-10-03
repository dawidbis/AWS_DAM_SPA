import { Component, computed, inject } from '@angular/core';
import { RouterLink, RouterLinkActive, RouterOutlet } from '@angular/router';

import { AuthService } from './core/auth/auth.service';
import { UserGroup } from './core/auth/user-group';

interface NavLink {
  path: string;
  label: string;
  groups: UserGroup[];
}

const NAV_LINKS: NavLink[] = [
  { path: '/gallery', label: 'Galeria', groups: ['admin', 'staff', 'viewer'] },
  { path: '/upload', label: 'Upload', groups: ['admin', 'contributor'] },
  { path: '/my-submissions', label: 'Moje zgłoszenia', groups: ['admin', 'contributor'] },
  { path: '/admin', label: 'Administracja', groups: ['admin'] },
];

@Component({
  imports: [RouterOutlet, RouterLink, RouterLinkActive],
  selector: 'app-root',
  styleUrl: './app.css',
  templateUrl: './app.html',
})
export class App {
  protected readonly auth = inject(AuthService);

  /** Linki widoczne dla grup użytkownika (UX; uprawnienia egzekwuje backend). */
  protected readonly links = computed(() =>
    NAV_LINKS.filter((link) => this.auth.hasAnyGroup(link.groups)),
  );
}
