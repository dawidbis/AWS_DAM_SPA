import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { of } from 'rxjs';

import { AssetsService } from '../../core/api/assets.service';
import { BROWSER_CONFIRM } from '../../core/api/delete-asset.service';
import { BROWSER_LOCATION } from '../../core/api/download.service';
import { asset } from '../../testing/assets';
import { FakeOidcSecurityService, provideFakeAuth } from '../../testing/fake-oidc';
import { GalleryPage } from './gallery-page';

describe('GalleryPage', () => {
  const assign = vi.fn();
  const confirm = vi.fn(() => true);

  const setup = async (groups: string[]) => {
    const oidc = new FakeOidcSecurityService();
    oidc.signIn({ 'cognito:groups': groups });
    const assets = {
      list: vi.fn(() =>
        of({ items: [asset('a1', { previewUrl: 'https://s3.example/p' })], nextCursor: null }),
      ),
      downloadUrl: vi.fn(() => of({ url: 'https://s3.example/d', expiresInSeconds: 300 })),
      publish: vi.fn(),
      remove: vi.fn((id: string) => of({ assetId: id })),
    };
    TestBed.configureTestingModule({
      imports: [GalleryPage],
      providers: [
        provideRouter([]),
        ...provideFakeAuth(oidc),
        { provide: AssetsService, useValue: assets },
        { provide: BROWSER_LOCATION, useValue: { assign } },
        { provide: BROWSER_CONFIRM, useValue: confirm },
      ],
    });
    const fixture = TestBed.createComponent(GalleryPage);
    await fixture.whenStable();
    return { fixture, assets, element: fixture.nativeElement as HTMLElement };
  };

  beforeEach(() => {
    assign.mockReset();
    confirm.mockReset().mockReturnValue(true);
  });

  const deleteButton = (element: HTMLElement) =>
    Array.from(element.querySelectorAll('button')).find((b) => b.textContent?.trim() === 'Usuń');

  it('lets admins delete an asset after confirmation', async () => {
    const { fixture, assets, element } = await setup(['admin']);
    deleteButton(element)?.click();
    await fixture.whenStable();

    expect(confirm).toHaveBeenCalledOnce();
    expect(assets.remove).toHaveBeenCalledWith('a1');
    expect(element.querySelectorAll('[data-testid="asset-card"]')).toHaveLength(0);
    expect(element.textContent).toContain('Usunięto');
  });

  it('does not delete when the admin cancels', async () => {
    const { fixture, assets, element } = await setup(['admin']);
    confirm.mockReturnValue(false);
    deleteButton(element)?.click();
    await fixture.whenStable();

    expect(assets.remove).not.toHaveBeenCalled();
    expect(element.querySelectorAll('[data-testid="asset-card"]')).toHaveLength(1);
  });

  it('shows no delete button to staff and viewers', async () => {
    expect(deleteButton((await setup(['staff'])).element)).toBeUndefined();
    TestBed.resetTestingModule();
    expect(deleteButton((await setup(['viewer'])).element)).toBeUndefined();
  });

  it('shows published assets with previews to staff and downloads via a presigned URL', async () => {
    const { fixture, assets, element } = await setup(['staff']);
    expect(assets.list).toHaveBeenCalledWith('gallery', null);
    expect(element.querySelector('img')?.getAttribute('src')).toBe('https://s3.example/p');

    element.querySelector<HTMLButtonElement>('[data-testid="asset-card"] button')?.click();
    await fixture.whenStable();

    expect(assets.downloadUrl).toHaveBeenCalledWith('a1');
    expect(assign).toHaveBeenCalledWith('https://s3.example/d');
  });

  it('does not query the API for contributors and points them to their submissions', async () => {
    const { assets, element } = await setup(['contributor']);
    expect(assets.list).not.toHaveBeenCalled();
    expect(element.querySelector('[data-testid="gallery-unavailable"] a')?.textContent).toContain(
      'Moje zgłoszenia',
    );
  });

  it('shows watermarked previews to viewers without a download button', async () => {
    const { assets, element } = await setup(['viewer']);
    expect(assets.list).toHaveBeenCalledWith('gallery', null);
    expect(element.querySelector('img')?.getAttribute('src')).toBe('https://s3.example/p');
    expect(element.querySelector('[data-testid="asset-card"] button')).toBeNull();
    expect(element.querySelector('[data-testid="watermark-info"]')).not.toBeNull();
  });
});
