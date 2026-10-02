import { loadRuntimeConfig } from './runtime-config';

describe('loadRuntimeConfig', () => {
  afterEach(() => vi.unstubAllGlobals());

  const stubFetch = (status: number, body: unknown) =>
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response(JSON.stringify(body), { status })),
    );

  it('returns a complete config', async () => {
    const config = {
      region: 'eu-central-1',
      authority: 'https://a',
      authDomain: 'https://d',
      clientId: 'c',
    };
    stubFetch(200, config);

    await expect(loadRuntimeConfig()).resolves.toEqual(config);
  });

  it('rejects a config with missing fields', async () => {
    stubFetch(200, { region: 'eu-central-1' });

    await expect(loadRuntimeConfig()).rejects.toThrow(/authority, authDomain, clientId/);
  });

  it('rejects when the file is missing', async () => {
    stubFetch(404, {});

    await expect(loadRuntimeConfig()).rejects.toThrow(/HTTP 404/);
  });
});
