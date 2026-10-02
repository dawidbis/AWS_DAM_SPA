import { TEST_RUNTIME_CONFIG } from '../../testing/fake-oidc';
import { buildOidcConfig } from './oidc-config';

describe('buildOidcConfig', () => {
  const origin = 'https://d123.cloudfront.net';

  it('uses the authorization code flow with redirects on the app origin', () => {
    const config = buildOidcConfig(TEST_RUNTIME_CONFIG, origin);

    expect(config.responseType).toBe('code');
    expect(config.clientId).toBe(TEST_RUNTIME_CONFIG.clientId);
    expect(config.authority).toBe(TEST_RUNTIME_CONFIG.authority);
    expect(config.redirectUrl).toBe(`${origin}/auth/callback`);
    expect(config.postLogoutRedirectUri).toBe(`${origin}/`);
  });

  it('reads user data from the ID token, which carries cognito:groups', () => {
    const config = buildOidcConfig(TEST_RUNTIME_CONFIG, origin);

    expect(config.autoUserInfo).toBe(false);
    expect(config.renewUserInfoAfterTokenRenew).toBe(true);
  });

  it('attaches tokens only to the API url', () => {
    expect(buildOidcConfig(TEST_RUNTIME_CONFIG, origin).secureRoutes).toEqual([]);

    const withApi = buildOidcConfig(
      { ...TEST_RUNTIME_CONFIG, apiUrl: 'https://api.example' },
      origin,
    );
    expect(withApi.secureRoutes).toEqual(['https://api.example']);
  });
});
