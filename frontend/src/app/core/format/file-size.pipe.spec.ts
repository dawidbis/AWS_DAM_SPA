import { formatFileSize } from './file-size.pipe';

describe('formatFileSize', () => {
  it.each([
    [0, '0 B'],
    [512, '512 B'],
    [1024, '1 KB'],
    [43_210, '42,2 KB'],
    [1_572_864, '1,5 MB'],
    [1_073_741_824, '1 GB'],
  ])('%d B -> %s', (bytes, expected) => {
    expect(formatFileSize(bytes).replace(/\s/g, ' ')).toBe(expected);
  });
});
