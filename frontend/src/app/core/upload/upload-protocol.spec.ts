import { WorkerEvent, partRange, uploadParts } from './upload-protocol';

describe('partRange', () => {
  it('splits a file into parts with a shorter last part', () => {
    expect(partRange(1, 10, 25)).toEqual({ start: 0, end: 10 });
    expect(partRange(3, 10, 25)).toEqual({ start: 20, end: 25 });
  });
});

describe('uploadParts', () => {
  const file = new Blob(['x'.repeat(25)]);
  const parts = [1, 2, 3].map((partNumber) => ({
    partNumber,
    url: `https://s3/part-${partNumber}`,
  }));

  const run = async (put: (url: string, body: Blob) => Promise<Response>) => {
    const events: WorkerEvent[] = [];
    await uploadParts(
      { type: 'upload', file, partSize: 10, parts, concurrency: 2 },
      (e) => events.push(e),
      put,
      2,
    );
    return events;
  };

  it('uploads every part with the right byte range', async () => {
    const sizes = new Map<string, number>();
    const events = await run(async (url, body) => {
      sizes.set(url, body.size);
      return new Response(null, { status: 200 });
    });

    expect(sizes).toEqual(
      new Map([
        ['https://s3/part-1', 10],
        ['https://s3/part-2', 10],
        ['https://s3/part-3', 5],
      ]),
    );
    expect(events.filter((e) => e.type === 'part-done')).toHaveLength(3);
    expect(events.at(-1)).toEqual({ type: 'done' });
  });

  it('retries transient failures', async () => {
    const attempts = new Map<string, number>();
    vi.useFakeTimers();
    const promise = run(async (url) => {
      const attempt = (attempts.get(url) ?? 0) + 1;
      attempts.set(url, attempt);
      return new Response(null, { status: attempt === 1 && url.endsWith('2') ? 500 : 200 });
    });
    await vi.runAllTimersAsync();
    const events = await promise;
    vi.useRealTimers();

    expect(attempts.get('https://s3/part-2')).toBe(2);
    expect(events.at(-1)).toEqual({ type: 'done' });
  });

  it('stops on an expired signature without retrying', async () => {
    let calls = 0;
    const events = await run(async () => {
      calls++;
      return new Response(null, { status: 403 });
    });

    expect(events.some((e) => e.type === 'error')).toBe(true);
    expect(events.some((e) => e.type === 'done')).toBe(false);
    expect(calls).toBeLessThanOrEqual(2);
  });
});
