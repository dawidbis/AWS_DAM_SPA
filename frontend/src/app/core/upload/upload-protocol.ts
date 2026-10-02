/**
 * Typy wiadomości między stroną a Web Workerem wysyłającym części pliku.
 * Wspólne dla obu stron, bez zależności od Angulara.
 */

/** Presigned URL do jednej części (z POST /uploads lub GET /uploads/{id}). */
export interface PartTarget {
  partNumber: number;
  url: string;
}

export interface UploadPartsCommand {
  type: 'upload';
  file: Blob;
  partSize: number;
  parts: PartTarget[];
  concurrency: number;
}

export type WorkerEvent =
  | { type: 'part-done'; partNumber: number; bytes: number }
  | { type: 'done' }
  | { type: 'error'; partNumber: number; message: string };

/** Zakres bajtów części `partNumber` (numeracja od 1, jak w S3). */
export function partRange(
  partNumber: number,
  partSize: number,
  fileSize: number,
): { start: number; end: number } {
  const start = (partNumber - 1) * partSize;
  return { start, end: Math.min(start + partSize, fileSize) };
}

/** Wysyła części równolegle (pula `concurrency`), ponawiając każdą do `retries` razy. */
export async function uploadParts(
  command: UploadPartsCommand,
  emit: (event: WorkerEvent) => void,
  put: (url: string, body: Blob) => Promise<Response> = (url, body) =>
    fetch(url, { method: 'PUT', body }),
  retries = 3,
): Promise<void> {
  const queue = [...command.parts];
  let failed = false;

  const uploadOne = async (part: PartTarget): Promise<void> => {
    const { start, end } = partRange(part.partNumber, command.partSize, command.file.size);
    const body = command.file.slice(start, end);
    for (let attempt = 1; attempt <= retries; attempt++) {
      try {
        const response = await put(part.url, body);
        if (response.ok) {
          emit({ type: 'part-done', partNumber: part.partNumber, bytes: end - start });
          return;
        }
        if (response.status === 403) {
          // Wygasły lub niepasujący podpis: ponowienie nic nie da, trzeba nowych URL-i.
          throw new Error(`HTTP 403 dla części ${part.partNumber}`);
        }
        if (attempt === retries) {
          throw new Error(`HTTP ${response.status} dla części ${part.partNumber}`);
        }
      } catch (error) {
        if (
          attempt === retries ||
          (error instanceof Error && error.message.startsWith('HTTP 403'))
        ) {
          throw error;
        }
      }
      await new Promise((resolve) => setTimeout(resolve, 500 * 2 ** attempt));
    }
  };

  const worker = async (): Promise<void> => {
    while (!failed && queue.length > 0) {
      const part = queue.shift();
      if (!part) {
        return;
      }
      try {
        await uploadOne(part);
      } catch (error) {
        failed = true;
        emit({
          type: 'error',
          partNumber: part.partNumber,
          message: error instanceof Error ? error.message : String(error),
        });
      }
    }
  };

  await Promise.all(Array.from({ length: Math.max(1, command.concurrency) }, () => worker()));
  if (!failed) {
    emit({ type: 'done' });
  }
}
