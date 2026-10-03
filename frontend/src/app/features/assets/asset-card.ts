import { DatePipe, DecimalPipe } from '@angular/common';
import { Component, input } from '@angular/core';

import { STATUS_PRESENTATION } from '../../core/api/asset-status';
import { AssetSummary } from '../../core/api/generated-types/AssetSummary';

/** Kafelek assetu z podglądem; akcje przekazuje rodzic przez <ng-content>. */
@Component({
  selector: 'app-asset-card',
  imports: [DatePipe, DecimalPipe],
  template: `
    @let item = asset();
    <article class="card bg-base-100 shadow-sm" data-testid="asset-card">
      <figure class="bg-base-300 aspect-video">
        @if (item.previewUrl) {
          <img
            class="h-full w-full object-cover"
            [src]="item.previewUrl"
            [alt]="item.title || item.originalFilename"
            loading="lazy"
            referrerpolicy="no-referrer"
          />
        } @else {
          <span class="text-sm opacity-60">{{ item.contentType }}</span>
        }
      </figure>
      <div class="card-body gap-1 p-4">
        <h3 class="card-title text-base break-all">{{ item.title || item.originalFilename }}</h3>
        <p class="text-xs opacity-70 break-all">
          {{ item.originalFilename }} · {{ item.sizeBytes / 1048576 | number: '1.0-1' }} MB ·
          {{ item.createdAt | date: 'short' }}
        </p>
        @if (showStatus()) {
          <span class="badge badge-sm {{ status[item.status].badge }}">
            {{ status[item.status].label }}
          </span>
        }
        <div class="card-actions mt-2 justify-end">
          <ng-content />
        </div>
      </div>
    </article>
  `,
})
export class AssetCard {
  readonly asset = input.required<AssetSummary>();
  readonly showStatus = input(false);
  protected readonly status = STATUS_PRESENTATION;
}
