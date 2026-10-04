import { DatePipe } from '@angular/common';
import { Component, inject, input } from '@angular/core';

import { CATEGORY_LABELS } from '../../core/api/asset-labels';
import { STATUS_PRESENTATION } from '../../core/api/asset-status';
import { DictionariesService } from '../../core/api/dictionaries.service';
import { AssetSummary } from '../../core/api/generated-types/AssetSummary';
import { FileSizePipe } from '../../core/format/file-size.pipe';

/** Kafelek assetu z podglądem; akcje przekazuje rodzic przez <ng-content>. */
@Component({
  selector: 'app-asset-card',
  imports: [DatePipe, FileSizePipe],
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
          {{ item.originalFilename }} · {{ item.sizeBytes | fileSize }} ·
          {{ item.createdAt | date: 'short' }}
        </p>
        @if (item.category || item.matchId || item.seasonId) {
          <p class="text-xs" data-testid="asset-context">
            @if (item.category) {
              <span class="badge badge-sm badge-outline">{{ categories[item.category] }}</span>
            }
            @if (item.matchId) {
              {{ dictionaries.matchLabel(item.matchId) }}
            } @else if (item.seasonId) {
              {{ dictionaries.seasonName(item.seasonId) }}
              @if (item.competitionId) {
                · {{ dictionaries.competitionName(item.competitionId) }}
              }
            }
          </p>
        }
        @if (item.playerIds.length) {
          <p class="text-xs opacity-80" data-testid="asset-players">
            {{ playerNames(item) }}
          </p>
        }
        @if (item.tags.length) {
          <div class="flex flex-wrap gap-1">
            @for (tag of item.tags; track tag) {
              <span class="badge badge-ghost badge-sm">#{{ tag }}</span>
            }
          </div>
        }
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
  protected readonly categories = CATEGORY_LABELS;
  protected readonly dictionaries = inject(DictionariesService);

  protected playerNames(item: AssetSummary): string {
    return item.playerIds.map((id) => this.dictionaries.playerName(id)).join(', ');
  }
}
