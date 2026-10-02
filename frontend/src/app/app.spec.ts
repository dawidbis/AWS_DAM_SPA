import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';

import { App } from './app';
import { FakeOidcSecurityService, provideFakeAuth } from './testing/fake-oidc';

describe('App', () => {
  let oidc: FakeOidcSecurityService;

  beforeEach(async () => {
    oidc = new FakeOidcSecurityService();
    await TestBed.configureTestingModule({
      imports: [App],
      providers: [provideRouter([]), ...provideFakeAuth(oidc)],
    }).compileComponents();
  });

  const navLabels = async () => {
    const fixture = TestBed.createComponent(App);
    await fixture.whenStable();
    const element = fixture.nativeElement as HTMLElement;
    return Array.from(element.querySelectorAll('.menu a')).map((a) => a.textContent?.trim());
  };

  it('shows a login button and no sections when anonymous', async () => {
    expect(await navLabels()).toEqual([]);
    const fixture = TestBed.createComponent(App);
    await fixture.whenStable();
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('Zaloguj');
  });

  it('shows only the gallery to a viewer', async () => {
    oidc.signIn({ email: 'sponsor@example.com', 'cognito:groups': ['viewer'] });
    expect(await navLabels()).toEqual(['Galeria']);
  });

  it('shows upload sections to a contributor', async () => {
    oidc.signIn({ 'cognito:groups': ['contributor'] });
    expect(await navLabels()).toEqual(['Galeria', 'Upload', 'Moje zgłoszenia']);
  });

  it('shows every section to an admin', async () => {
    oidc.signIn({ 'cognito:groups': ['admin'] });
    expect(await navLabels()).toEqual(['Galeria', 'Upload', 'Moje zgłoszenia', 'Administracja']);
  });
});
