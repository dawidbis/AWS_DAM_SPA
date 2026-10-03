import { signal } from '@angular/core';
import { Observable } from 'rxjs';

import { AssetListResponse } from './generated-types/AssetListResponse';
import { AssetSummary } from './generated-types/AssetSummary';

export type PagerState = 'idle' | 'loading' | 'ready' | 'error';

/** Stronicowana lista assetów z kursorem z API. */
export class AssetPager {
  readonly items = signal<AssetSummary[]>([]);
  readonly state = signal<PagerState>('idle');
  readonly nextCursor = signal<string | null>(null);

  constructor(private readonly fetch: (cursor: string | null) => Observable<AssetListResponse>) {}

  /** Pierwsza strona (odświeżenie listy). */
  reload(): void {
    this.load(null, true);
  }

  more(): void {
    const cursor = this.nextCursor();
    if (cursor && this.state() !== 'loading') {
      this.load(cursor, false);
    }
  }

  remove(assetId: string): void {
    this.items.update((items) => items.filter((item) => item.assetId !== assetId));
  }

  private load(cursor: string | null, replace: boolean): void {
    this.state.set('loading');
    this.fetch(cursor).subscribe({
      next: (page) => {
        this.items.update((items) => (replace ? page.items : [...items, ...page.items]));
        this.nextCursor.set(page.nextCursor);
        this.state.set('ready');
      },
      error: () => this.state.set('error'),
    });
  }
}
