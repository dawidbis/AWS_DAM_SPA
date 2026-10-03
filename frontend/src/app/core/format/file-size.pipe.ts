import { Pipe, PipeTransform } from '@angular/core';

const UNITS = ['B', 'KB', 'MB', 'GB'] as const;
const NUMBER = new Intl.NumberFormat('pl-PL', { maximumFractionDigits: 1 });

/** Rozmiar pliku w czytelnej jednostce (1024 B = 1 KB), np. „1,5 MB”. */
export function formatFileSize(bytes: number): string {
  let value = Math.max(0, bytes);
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${NUMBER.format(unit === 0 ? Math.round(value) : value)} ${UNITS[unit]}`;
}

@Pipe({ name: 'fileSize' })
export class FileSizePipe implements PipeTransform {
  transform(bytes: number): string {
    return formatFileSize(bytes);
  }
}
