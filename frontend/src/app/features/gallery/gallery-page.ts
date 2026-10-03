import { Component, inject } from '@angular/core';
import { RouterLink } from '@angular/router';

import { AssetPager } from '../../core/api/asset-pager';
import { AssetsService } from '../../core/api/assets.service';
import { DownloadService } from '../../core/api/download.service';
import { AuthService } from '../../core/auth/auth.service';
import { AssetCard } from '../assets/asset-card';

/** Galeria opublikowanych materiałów: A i B (podgląd i pobieranie oryginału). */
@Component({
  selector: 'app-gallery-page',
  imports: [AssetCard, RouterLink],
  template: `
    <div class="mb-4 flex items-center justify-between gap-2">
      <h2 class="text-2xl font-bold">Galeria</h2>
      @if (canBrowse) {
        <button class="btn btn-sm" type="button" (click)="pager.reload()">Odśwież</button>
      }
    </div>

    @if (!canBrowse) {
      <div class="alert" data-testid="gallery-unavailable">
        @if (auth.hasAnyGroup(['contributor'])) {
          <span>
            Jako fotograf widzisz tylko własne materiały:
            <a class="link" routerLink="/my-submissions">Moje zgłoszenia</a>.
          </span>
        } @else {
          <span>Podglądy z watermarkiem dla partnerów pojawią się w kolejnym etapie.</span>
        }
      </div>
    } @else {
      @if (downloads.error(); as error) {
        <div class="alert alert-error mb-4">{{ error }}</div>
      }
      @switch (pager.state()) {
        @case ('error') {
          <div class="alert alert-error">Nie udało się wczytać galerii.</div>
        }
        @default {
          <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
            @for (asset of pager.items(); track asset.assetId) {
              <app-asset-card [asset]="asset">
                <button
                  class="btn btn-sm btn-primary"
                  type="button"
                  [disabled]="downloads.pending() === asset.assetId"
                  (click)="downloads.start(asset.assetId)"
                >
                  Pobierz
                </button>
              </app-asset-card>
            } @empty {
              @if (pager.state() === 'ready') {
                <p class="opacity-70">Brak opublikowanych materiałów.</p>
              }
            }
          </div>
          @if (pager.state() === 'loading') {
            <span class="loading loading-spinner mt-4"></span>
          } @else if (pager.nextCursor()) {
            <button class="btn btn-sm mt-4" type="button" (click)="pager.more()">
              Załaduj więcej
            </button>
          }
        }
      }
    }
  `,
})
export class GalleryPage {
  protected readonly auth = inject(AuthService);
  protected readonly downloads = inject(DownloadService);
  private readonly assets = inject(AssetsService);

  protected readonly canBrowse = this.auth.hasAnyGroup(['admin', 'staff']);
  protected readonly pager = new AssetPager((cursor) => this.assets.list('gallery', cursor));

  constructor() {
    if (this.canBrowse) {
      this.pager.reload();
    }
  }
}
