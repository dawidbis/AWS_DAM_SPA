import { TestBed } from '@angular/core/testing';

import { FakeOidcSecurityService, provideFakeAuth } from '../../testing/fake-oidc';
import { AuthService } from './auth.service';

describe('AuthService', () => {
  let oidc: FakeOidcSecurityService;
  let auth: AuthService;

  beforeEach(() => {
    oidc = new FakeOidcSecurityService();
    TestBed.configureTestingModule({ providers: provideFakeAuth(oidc) });
    auth = TestBed.inject(AuthService);
    sessionStorage.clear();
  });

  it('is anonymous before login', () => {
    expect(auth.isAuthenticated()).toBe(false);
    expect(auth.groups()).toEqual([]);
    expect(auth.hasAnyGroup(['admin'])).toBe(false);
  });

  it('reads known groups from cognito:groups and ignores unknown ones', () => {
    oidc.signIn({ email: 'foto@example.com', 'cognito:groups': ['contributor', 'root', 42] });

    expect(auth.isAuthenticated()).toBe(true);
    expect(auth.email()).toBe('foto@example.com');
    expect(auth.groups()).toEqual(['contributor']);
    expect(auth.hasAnyGroup(['admin', 'contributor'])).toBe(true);
    expect(auth.hasAnyGroup(['admin'])).toBe(false);
  });

  it('remembers the return url and starts authorization', () => {
    auth.login('/upload');

    expect(oidc.authorize).toHaveBeenCalledOnce();
    expect(auth.consumeReturnUrl()).toBe('/upload');
    expect(auth.consumeReturnUrl()).toBe('/');
  });

  it.each(['https://evil.example', '//evil.example', 'javascript:alert(1)'])(
    'does not return to external url %s',
    (url) => {
      auth.login(url);
      expect(auth.consumeReturnUrl()).toBe('/');
    },
  );
});
