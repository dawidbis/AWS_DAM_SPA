/// <reference lib="webworker" />

// Wysyłanie części pliku poza wątkiem UI: przeglądarka pozostaje płynna
// nawet przy pliku 1 GB. Plik idzie bezpośrednio do S3 (presigned PUT),
// bez tokenu JWT i bez przechodzenia przez API.

import { UploadPartsCommand, uploadParts } from './upload-protocol';

addEventListener('message', ({ data }: MessageEvent<UploadPartsCommand>) => {
  if (data.type === 'upload') {
    void uploadParts(data, (event) => postMessage(event));
  }
});
