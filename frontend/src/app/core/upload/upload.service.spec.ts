import { TestBed } from '@angular/core/testing';
import { of, throwError } from 'rxjs';
import { HttpErrorResponse } from '@angular/common/http';

import { PendingUpload, PendingUploadsStore } from './pending-uploads.store';
import { UploadApiService } from './upload-api.service';
import { UploadPartsCommand, WorkerEvent, uploadParts } from './upload-protocol';
import { UploadService } from './upload.service';

class MemoryStore {
  items = new Map<string, PendingUpload>();
  async list() {
    return [...this.items.values()];
  }
  async save(upload: PendingUpload) {
    this.items.set(upload.assetId, upload);
  }
  async remove(assetId: string) {
    this.items.delete(assetId);
  }
  async findFor(file: File) {
    return [...this.items.values()].find((u) => u.fileName === file.name && u.size === file.size);
  }
}

/** Worker wykonujący uploadParts w tym samym wątku, z podmienionym PUT. */
function inlineWorker(put: (url: string) => number, uploaded: string[]): () => Worker {
  return () => {
    const worker = {
      onmessage: null as ((event: MessageEvent<WorkerEvent>) => void) | null,
      onerror: null,
      terminate: vi.fn(),
      postMessage(command: UploadPartsCommand) {
        void uploadParts(
          command,
          (event) => worker.onmessage?.({ data: event } as MessageEvent<WorkerEvent>),
          async (url) => {
            uploaded.push(url);
            return new Response(null, { status: put(url) });
          },
          1,
        );
      },
    };
    return worker as unknown as Worker;
  };
}

describe('UploadService', () => {
  let api: {
    init: ReturnType<typeof vi.fn>;
    status: ReturnType<typeof vi.fn>;
    complete: ReturnType<typeof vi.fn>;
  };
  let store: MemoryStore;
  let service: UploadService;
  let uploaded: string[];

  const file = new File(['x'.repeat(25)], 'mecz.jpg', { type: 'image/jpeg', lastModified: 1 });

  beforeEach(() => {
    api = { init: vi.fn(), status: vi.fn(), complete: vi.fn() };
    store = new MemoryStore();
    uploaded = [];
    TestBed.configureTestingModule({
      providers: [
        { provide: UploadApiService, useValue: api },
        { provide: PendingUploadsStore, useValue: store },
      ],
    });
    service = TestBed.inject(UploadService);
    service.createWorker = inlineWorker(() => 200, uploaded);
  });

  it('rejects unsupported types before calling the API', async () => {
    await service.upload(new File(['<svg/>'], 'a.svg', { type: 'image/svg+xml' }), null);

    expect(service.state().phase).toBe('error');
    expect(api.init).not.toHaveBeenCalled();
  });

  it('uploads all parts and completes', async () => {
    api.init.mockReturnValue(
      of({
        assetId: 'a1',
        partSize: 10,
        partCount: 3,
        urlsExpireInSeconds: 3600,
        parts: [1, 2, 3].map((n) => ({ partNumber: n, url: `https://s3/${n}` })),
      }),
    );
    api.complete.mockReturnValue(of({ assetId: 'a1', status: 'QUARANTINED' }));

    await service.upload(file, 'Gol w 90. minucie');

    expect(api.init).toHaveBeenCalledWith(file, 'Gol w 90. minucie');
    expect(uploaded.sort()).toEqual(['https://s3/1', 'https://s3/2', 'https://s3/3']);
    expect(service.state()).toMatchObject({ phase: 'done', uploadedBytes: 25, assetId: 'a1' });
    expect(await store.list()).toEqual([]);
  });

  it('keeps the upload resumable when a part fails', async () => {
    service.createWorker = inlineWorker((url) => (url.endsWith('/2') ? 500 : 200), uploaded);
    api.init.mockReturnValue(
      of({
        assetId: 'a1',
        partSize: 10,
        partCount: 3,
        urlsExpireInSeconds: 3600,
        parts: [1, 2, 3].map((n) => ({ partNumber: n, url: `https://s3/${n}` })),
      }),
    );

    await service.upload(file, null);

    expect(service.state().phase).toBe('error');
    expect(api.complete).not.toHaveBeenCalled();
    expect((await store.list()).map((u) => u.assetId)).toEqual(['a1']);
  });

  it('resumes only the missing parts of the same file', async () => {
    await store.save({
      assetId: 'a1',
      fileName: 'mecz.jpg',
      size: 25,
      lastModified: 1,
      createdAt: 0,
    });
    api.status.mockReturnValue(
      of({
        assetId: 'a1',
        status: 'UPLOADING',
        partSize: 10,
        partCount: 3,
        uploadedParts: [1, 3],
        parts: [{ partNumber: 2, url: 'https://s3/2-fresh' }],
        urlsExpireInSeconds: 3600,
      }),
    );
    api.complete.mockReturnValue(of({ assetId: 'a1', status: 'QUARANTINED' }));

    await service.upload(file, null);

    expect(api.init).not.toHaveBeenCalled();
    expect(uploaded).toEqual(['https://s3/2-fresh']);
    expect(service.state()).toMatchObject({ phase: 'done', resumed: true });
  });

  it('shows the API message when the server rejects the upload', async () => {
    api.init.mockReturnValue(
      throwError(() => new HttpErrorResponse({ status: 403, error: { message: 'Forbidden' } })),
    );

    await service.upload(file, null);

    expect(service.state()).toMatchObject({ phase: 'error', message: 'Forbidden' });
  });
});
