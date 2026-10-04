import { AssetCategory } from './generated-types/AssetCategory';
import { Position } from './generated-types/Position';

/**
 * Nazwy kategorii (rozdział 7.4). `Record<AssetCategory, …>` wymusza obsługę
 * każdej kategorii dodanej w Ruście (typ generowany przez ts-rs).
 */
export const CATEGORY_LABELS: Record<AssetCategory, string> = {
  MATCH_PHOTO: 'Zdjęcia meczowe',
  TRAINING_PHOTO: 'Zdjęcia treningowe',
  VIDEO: 'Wideo',
  BRAND_IDENTITY: 'Identyfikacja wizualna',
  SPONSOR_MATERIAL: 'Materiały sponsorskie',
  PRESS_DOCUMENT: 'Dokumenty prasowe',
};

export const CATEGORIES = Object.keys(CATEGORY_LABELS) as AssetCategory[];

export const POSITION_LABELS: Record<Position, string> = {
  GOALKEEPER: 'Bramkarz',
  DEFENDER: 'Obrońca',
  MIDFIELDER: 'Pomocnik',
  FORWARD: 'Napastnik',
};

export const POSITIONS = Object.keys(POSITION_LABELS) as Position[];
