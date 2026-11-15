import { registerLocaleData } from '@angular/common';
import localeFr from '@angular/common/locales/fr';
import { loadTranslations } from '@angular/localize';
import { bootstrapApplication } from '@angular/platform-browser';
import { LANG_KEY } from '@components/language-toggle/language-toggle';
import { appConfig } from './app/app.config';
import { App } from './app/app';

// Translations are loaded at runtime (no per-locale builds): switching language reloads the app.
const lang = localStorage.getItem(LANG_KEY) ?? (navigator.language.startsWith('fr') ? 'fr' : 'en');
if (lang === 'fr') {
  const { translations } = await import('./locale/messages.fr.json');
  loadTranslations(translations);
  registerLocaleData(localeFr);
  $localize.locale = 'fr'; // LOCALE_ID defaults to this
}

bootstrapApplication(App, appConfig).catch((err) => console.error(err));
