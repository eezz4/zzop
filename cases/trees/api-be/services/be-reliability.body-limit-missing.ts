// be-reliability/body-limit-missing — bad: a body parser with no explicit limit. good: an explicit limit.
import express from 'express';
type App = ReturnType<typeof express>;

export function bad(app: App) {
  app.use(express.json());
}

export function good(app: App) {
  app.use(express.json({ limit: '1mb' }));
}
