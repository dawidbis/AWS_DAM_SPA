import { Routes } from '@angular/router';

import { AuthCallback } from './core/auth/auth-callback';
import { requireGroups } from './core/auth/require-groups.guard';
import { AdminPage } from './features/admin/admin-page';
import { GalleryPage } from './features/gallery/gallery-page';
import { HomePage } from './features/pages/home-page';
import { PlaceholderPage } from './features/pages/placeholder-page';
import { MySubmissionsPage } from './features/submissions/my-submissions-page';
import { UploadPage } from './features/upload/upload-page';

export const routes: Routes = [
  { path: '', component: HomePage, title: 'Matchday DAM' },
  { path: 'auth/callback', component: AuthCallback },
  {
    path: 'gallery',
    component: GalleryPage,
    canActivate: [requireGroups('admin', 'staff', 'contributor', 'viewer')],
    title: 'Galeria',
  },
  {
    path: 'upload',
    component: UploadPage,
    canActivate: [requireGroups('admin', 'contributor')],
    title: 'Upload',
  },
  {
    path: 'my-submissions',
    component: MySubmissionsPage,
    canActivate: [requireGroups('admin', 'contributor')],
    title: 'Moje zgłoszenia',
  },
  {
    path: 'admin',
    component: AdminPage,
    canActivate: [requireGroups('admin')],
    title: 'Administracja',
  },
  {
    path: 'forbidden',
    component: PlaceholderPage,
    title: 'Brak dostępu',
    data: { title: 'Brak dostępu', description: 'Twoja grupa nie ma dostępu do tej sekcji.' },
  },
  { path: '**', redirectTo: '' },
];
