import { TestBed } from '@angular/core/testing';
import {
  ActivatedRouteSnapshot,
  Router,
  RouterStateSnapshot,
  UrlTree,
  provideRouter,
} from '@angular/router';

import { FakeOidcSecurityService, provideFakeAuth } from '../../testing/fake-oidc';
import { requireGroups } from './require-groups.guard';

describe('requireGroups', () => {
  let oidc: FakeOidcSecurityService;

  const run = (url: string) =>
    TestBed.runInInjectionContext(() =>
      requireGroups('admin')({} as ActivatedRouteSnapshot, { url } as RouterStateSnapshot),
    );

  beforeEach(() => {
    oidc = new FakeOidcSecurityService();
    TestBed.configureTestingModule({ providers: [provideRouter([]), ...provideFakeAuth(oidc)] });
  });

  it('sends anonymous users to login', () => {
    expect(run('/admin')).toBe(false);
    expect(oidc.authorize).toHaveBeenCalledOnce();
  });

  it('redirects users without the group to /forbidden', () => {
    oidc.signIn({ 'cognito:groups': ['viewer'] });

    const result = run('/admin');

    expect(result).toBeInstanceOf(UrlTree);
    expect(TestBed.inject(Router).serializeUrl(result as UrlTree)).toBe('/forbidden');
  });

  it('lets members of the group in', () => {
    oidc.signIn({ 'cognito:groups': ['admin'] });
    expect(run('/admin')).toBe(true);
  });
});
