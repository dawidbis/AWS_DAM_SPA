import { HttpClient } from '@angular/common/http';
import { Injectable, inject } from '@angular/core';
import { Observable } from 'rxjs';

import { RUNTIME_CONFIG } from '../config/runtime-config';
import { UserGroup } from '../auth/user-group';

/** Odpowiedź `GET /me`: tożsamość wywołującego widziana przez backend. */
export interface MeResponse {
  sub: string;
  email: string | null;
  groups: UserGroup[];
}

@Injectable({ providedIn: 'root' })
export class MeService {
  private readonly http = inject(HttpClient);
  private readonly config = inject(RUNTIME_CONFIG);

  /** Czy środowisko ma skonfigurowane API (config.json → apiUrl). */
  readonly available = Boolean(this.config.apiUrl);

  /** Token JWT dokleja interceptor (tylko dla adresów z apiUrl). */
  get(): Observable<MeResponse> {
    return this.http.get<MeResponse>(`${this.config.apiUrl}/me`);
  }
}
