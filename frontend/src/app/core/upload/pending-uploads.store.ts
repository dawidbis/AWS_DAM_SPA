import { Injectable } from '@angular/core';

/** Niedokończony upload zapamiętany w IndexedDB, żeby można go było wznowić. */
export interface PendingUpload {
  assetId: string;
  fileName: string;
  size: number;
  lastModified: number;
  createdAt: number;
}

const DB_NAME = 'matchday-dam';
const STORE = 'pending-uploads';

/**
 * Lista niedokończonych uploadów. Stan części trzyma S3 (ListParts), tu tylko
 * powiązanie pliku z assetId. Bez IndexedDB (np. tryb prywatny) wznawianie
 * po prostu nie jest dostępne.
 */
@Injectable({ providedIn: 'root' })
export class PendingUploadsStore {
  private db: Promise<IDBDatabase | null> | null = null;

  private open(): Promise<IDBDatabase | null> {
    this.db ??= new Promise((resolve) => {
      if (typeof indexedDB === 'undefined') {
        resolve(null);
        return;
      }
      const request = indexedDB.open(DB_NAME, 1);
      request.onupgradeneeded = () =>
        request.result.createObjectStore(STORE, { keyPath: 'assetId' });
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => resolve(null);
    });
    return this.db;
  }

  private async run<T>(
    mode: IDBTransactionMode,
    action: (store: IDBObjectStore) => IDBRequest<T>,
  ): Promise<T | null> {
    const db = await this.open();
    if (!db) {
      return null;
    }
    return new Promise((resolve) => {
      const request = action(db.transaction(STORE, mode).objectStore(STORE));
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => resolve(null);
    });
  }

  async list(): Promise<PendingUpload[]> {
    return (
      ((await this.run('readonly', (store) => store.getAll())) as PendingUpload[] | null) ?? []
    );
  }

  async save(upload: PendingUpload): Promise<void> {
    await this.run('readwrite', (store) => store.put(upload));
  }

  async remove(assetId: string): Promise<void> {
    await this.run('readwrite', (store) => store.delete(assetId));
  }

  /** Niedokończony upload tego samego pliku (nazwa, rozmiar i data modyfikacji). */
  async findFor(file: File): Promise<PendingUpload | undefined> {
    return (await this.list()).find(
      (upload) =>
        upload.fileName === file.name &&
        upload.size === file.size &&
        upload.lastModified === file.lastModified,
    );
  }
}
