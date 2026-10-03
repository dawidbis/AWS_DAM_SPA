import { DecimalPipe } from '@angular/common';
import { RouterLink } from '@angular/router';
import { Component, computed, inject, signal } from '@angular/core';

import { ACCEPTED_TYPES, UploadService } from '../../core/upload/upload.service';

@Component({
  selector: 'app-upload-page',
  imports: [DecimalPipe, RouterLink],
  template: `
    <section class="card bg-base-200 max-w-2xl">
      <div class="card-body gap-4">
        <h2 class="card-title">Upload</h2>
        <p class="text-sm opacity-80">
          Zdjęcia JPEG, PNG lub WebP do 200 MB. Plik trafia do kwarantanny i jest sprawdzany przed
          publikacją. Przerwany upload wznowisz, wybierając ten sam plik ponownie.
        </p>

        <label class="form-control">
          <span class="label-text">Plik</span>
          <input
            type="file"
            class="file-input file-input-bordered w-full"
            [accept]="accept"
            [disabled]="busy()"
            (change)="onFile($event)"
          />
        </label>

        <label class="form-control">
          <span class="label-text">Tytuł (opcjonalnie)</span>
          <input
            type="text"
            class="input input-bordered w-full"
            maxlength="120"
            [disabled]="busy()"
            [value]="title()"
            (input)="title.set($any($event.target).value)"
          />
        </label>

        <button
          class="btn btn-primary self-start"
          type="button"
          [disabled]="!file() || busy()"
          (click)="start()"
        >
          {{ state().phase === 'error' && file() ? 'Wznów / spróbuj ponownie' : 'Wyślij' }}
        </button>

        @if (state().phase !== 'idle') {
          <div class="flex flex-col gap-2" data-testid="upload-status">
            <progress
              class="progress progress-primary w-full"
              [value]="percent()"
              max="100"
            ></progress>
            <span class="text-sm">
              {{ state().fileName }}: {{ percent() | number: '1.0-0' }}%
              @if (state().resumed) {
                (wznowiony)
              }
            </span>
            @if (state().message) {
              <div
                class="alert"
                [class.alert-success]="state().phase === 'done'"
                [class.alert-error]="state().phase === 'error'"
              >
                {{ state().message }}
              </div>
            }
            @if (state().phase === 'done') {
              <a class="link text-sm" routerLink="/my-submissions">
                Sprawdź wynik skanu w „Moich zgłoszeniach”
              </a>
            }
          </div>
        }
      </div>
    </section>
  `,
})
export class UploadPage {
  private readonly uploads = inject(UploadService);

  protected readonly accept = ACCEPTED_TYPES.join(',');
  protected readonly state = this.uploads.state;
  protected readonly file = signal<File | null>(null);
  protected readonly title = signal('');

  protected readonly busy = computed(() =>
    ['starting', 'uploading', 'completing'].includes(this.state().phase),
  );
  protected readonly percent = computed(() => {
    const { uploadedBytes, totalBytes } = this.state();
    return totalBytes > 0 ? (uploadedBytes / totalBytes) * 100 : 0;
  });

  protected onFile(event: Event): void {
    const input = event.target as HTMLInputElement;
    this.file.set(input.files?.item(0) ?? null);
    this.uploads.reset();
  }

  protected start(): void {
    const file = this.file();
    if (file) {
      void this.uploads.upload(file, this.title().trim() || null);
    }
  }
}
