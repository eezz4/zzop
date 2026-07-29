import * as client from './services/client';
import * as articles from './pages/articles';
import * as loader from './server/loader';
import * as settings from './settings';

export const registry = { client, articles, loader, settings };
