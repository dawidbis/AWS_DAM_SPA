import { Routes } from '@angular/router';

import { AuthCallback } from './core/auth/auth-callback';
import { requireGroups } from './core/auth/require-groups.guard';
import { HomePage } from './features/pages/home-page';
import { PlaceholderPage } from './features/pages/placeholder-page';
import { UploadPage } from './features/upload/upload-page';

export const routes: Routes = [
  { path: '', component: HomePage, title: 'Matchday DAM' },
  { path: 'auth/callback', component: AuthCallback },
  {
    path: 'gallery',
    component: PlaceholderPage,
    canActivate: [requireGroups('admin', 'staff', 'contributor', 'viewer')],
    title: 'Galeria',
    data: { title: 'Galeria', description: 'Opublikowane materiały. Powstaje w etapie 1.' },
  },
  {
    path: 'upload',
    component: UploadPage,
    canActivate: [requireGroups('admin', 'contributor')],
    title: 'Upload',
  },
  {
    path: 'my-submissions',
    component: PlaceholderPage,
    canActivate: [requireGroups('admin', 'contributor')],
    title: 'Moje zgłoszenia',
    data: { title: 'Moje zgłoszenia', description: 'Status wgranych plików. Powstaje w etapie 1.' },
  },
  {
    path: 'admin',
    component: PlaceholderPage,
    canActivate: [requireGroups('admin')],
    title: 'Administracja',
    data: { title: 'Administracja', description: 'Publikacja, kwarantanna i incydenty.' },
  },
  {
    path: 'forbidden',
    component: PlaceholderPage,
    title: 'Brak dostępu',
    data: { title: 'Brak dostępu', description: 'Twoja grupa nie ma dostępu do tej sekcji.' },
  },
  { path: '**', redirectTo: '' },
];
