import { DOCUMENT } from '@angular/common';
import { Injectable, InjectionToken, inject, signal } from '@angular/core';

import { AssetsService } from './assets.service';

/** Nawigacja przeglądarki (podmieniana w testach). */
export const BROWSER_LOCATION = new InjectionToken<Pick<Location, 'assign'>>('BROWSER_LOCATION', {
  providedIn: 'root',
  factory: () => inject(DOCUMENT).location,
});

/**
 * Pobieranie oryginału: API sprawdza uprawnienia i zwraca presigned URL
 * z `Content-Disposition: attachment`, więc przejście pod ten adres
 * zapisuje plik i nie opuszcza aplikacji.
 */
@Injectable({ providedIn: 'root' })
export class DownloadService {
  private readonly assets = inject(AssetsService);
  private readonly location = inject(BROWSER_LOCATION);

  /** Identyfikator assetu, dla którego trwa pobieranie linku. */
  readonly pending = signal<string | null>(null);
  readonly error = signal<string | null>(null);

  start(assetId: string): void {
    this.pending.set(assetId);
    this.error.set(null);
    this.assets.downloadUrl(assetId).subscribe({
      next: ({ url }) => {
        this.pending.set(null);
        this.location.assign(url);
      },
      error: () => {
        this.pending.set(null);
        this.error.set('Nie udało się pobrać pliku. Spróbuj ponownie.');
      },
    });
  }
}
