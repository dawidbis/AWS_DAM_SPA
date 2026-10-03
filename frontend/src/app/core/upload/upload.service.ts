import { HttpErrorResponse } from '@angular/common/http';
import { Injectable, inject, signal } from '@angular/core';
import { firstValueFrom } from 'rxjs';

import { PendingUploadsStore } from './pending-uploads.store';
import { UploadApiService } from './upload-api.service';
import { PartTarget, UploadPartsCommand, WorkerEvent } from './upload-protocol';

export type UploadPhase = 'idle' | 'starting' | 'uploading' | 'completing' | 'done' | 'error';

export interface UploadState {
  phase: UploadPhase;
  fileName: string | null;
  assetId: string | null;
  uploadedBytes: number;
  totalBytes: number;
  resumed: boolean;
  message: string | null;
}

const IDLE: UploadState = {
  phase: 'idle',
  fileName: null,
  assetId: null,
  uploadedBytes: 0,
  totalBytes: 0,
  resumed: false,
  message: null,
};

/** Liczba części wysyłanych równolegle. */
const CONCURRENCY = 4;

/** Typy, które przyjmuje backend (walidacja tu to tylko UX, rozdział 3.3). */
export const ACCEPTED_TYPES = ['image/jpeg', 'image/png', 'image/webp'];
export const MAX_FILE_BYTES = 200 * 1024 * 1024;

/**
 * Orkiestracja uploadu multipart: POST /uploads → części z Web Workera →
 * POST /complete. Przerwany upload tego samego pliku jest wznawiany:
 * GET /uploads/{id} zwraca brakujące części ze świeżymi URL-ami.
 */
@Injectable({ providedIn: 'root' })
export class UploadService {
  private readonly api = inject(UploadApiService);
  private readonly pending = inject(PendingUploadsStore);

  readonly state = signal<UploadState>(IDLE);

  /** Fabryka workera; podmieniana w testach. */
  createWorker: () => Worker = () =>
    new Worker(new URL('./upload.worker', import.meta.url), { type: 'module' });

  validate(file: File): string | null {
    if (!ACCEPTED_TYPES.includes(file.type)) {
      return 'Dozwolone są tylko zdjęcia JPEG, PNG i WebP.';
    }
    if (file.size === 0) {
      return 'Plik jest pusty.';
    }
    if (file.size > MAX_FILE_BYTES) {
      return 'Plik jest większy niż 200 MB.';
    }
    return null;
  }

  async upload(file: File, title: string | null): Promise<void> {
    const invalid = this.validate(file);
    if (invalid) {
      this.state.set({ ...IDLE, phase: 'error', fileName: file.name, message: invalid });
      return;
    }
    this.state.set({ ...IDLE, phase: 'starting', fileName: file.name, totalBytes: file.size });

    try {
      const { assetId, partSize, parts, uploadedBytes, resumed } = await this.prepare(file, title);
      this.state.update((s) => ({ ...s, phase: 'uploading', assetId, uploadedBytes, resumed }));

      if (parts.length > 0) {
        await this.sendParts(file, partSize, parts);
      }

      this.state.update((s) => ({ ...s, phase: 'completing' }));
      const result = await firstValueFrom(this.api.complete(assetId));
      await this.pending.remove(assetId);
      this.state.update((s) => ({
        ...s,
        phase: 'done',
        uploadedBytes: file.size,
        message: `Plik trafił do kwarantanny (status ${result.status}). Zostanie sprawdzony przed publikacją.`,
      }));
    } catch (error) {
      this.state.update((s) => ({ ...s, phase: 'error', message: describe(error) }));
    }
  }

  /** Nowy upload albo wznowienie niedokończonego uploadu tego samego pliku. */
  private async prepare(
    file: File,
    title: string | null,
  ): Promise<{
    assetId: string;
    partSize: number;
    parts: PartTarget[];
    uploadedBytes: number;
    resumed: boolean;
  }> {
    const previous = await this.pending.findFor(file);
    if (previous) {
      try {
        const status = await firstValueFrom(this.api.status(previous.assetId));
        if (status.status === 'UPLOADING') {
          const uploadedBytes = status.uploadedParts.reduce(
            (sum, part) =>
              sum + Math.min(status.partSize, file.size - (part - 1) * status.partSize),
            0,
          );
          return {
            assetId: status.assetId,
            partSize: status.partSize,
            parts: status.parts,
            uploadedBytes,
            resumed: true,
          };
        }
      } catch {
        // Nieaktualny wpis (np. upload wygasł): zaczynamy od nowa.
      }
      await this.pending.remove(previous.assetId);
    }

    const init = await firstValueFrom(this.api.init(file, title));
    await this.pending.save({
      assetId: init.assetId,
      fileName: file.name,
      size: file.size,
      lastModified: file.lastModified,
      createdAt: Date.now(),
    });
    return {
      assetId: init.assetId,
      partSize: init.partSize,
      parts: init.parts,
      uploadedBytes: 0,
      resumed: false,
    };
  }

  private sendParts(file: File, partSize: number, parts: PartTarget[]): Promise<void> {
    return new Promise((resolve, reject) => {
      const worker = this.createWorker();
      worker.onmessage = ({ data }: MessageEvent<WorkerEvent>) => {
        if (data.type === 'part-done') {
          this.state.update((s) => ({ ...s, uploadedBytes: s.uploadedBytes + data.bytes }));
        } else if (data.type === 'done') {
          worker.terminate();
          resolve();
        } else {
          worker.terminate();
          reject(new Error(`Nie udało się wysłać części ${data.partNumber}: ${data.message}`));
        }
      };
      worker.onerror = (event) => {
        worker.terminate();
        reject(new Error(event.message || 'Błąd Web Workera'));
      };
      const command: UploadPartsCommand = {
        type: 'upload',
        file,
        partSize,
        parts,
        concurrency: CONCURRENCY,
      };
      worker.postMessage(command);
    });
  }

  reset(): void {
    this.state.set(IDLE);
  }
}

function describe(error: unknown): string {
  if (error instanceof HttpErrorResponse) {
    const message = (error.error as { message?: string } | null)?.message;
    return message ?? `Błąd API (HTTP ${error.status}).`;
  }
  return error instanceof Error
    ? `${error.message}. Wybierz ten sam plik ponownie, aby wznowić upload.`
    : 'Nieznany błąd.';
}
