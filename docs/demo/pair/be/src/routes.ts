// The BACKEND half of the break-a-route demo. See ../../../break-a-route.md.
//
// Note where the prefix is and is not. The route strings below do NOT carry it; it is applied once,
// at the mount, at the bottom of this file. So the full path the frontend calls appears nowhere in
// this tree as text -- the demo runs that search in front of you before it runs the join. (This
// comment deliberately does not spell that path either: the demo asserts the search finds zero, and
// a comment quoting it would make the file lie. That assertion has already caught one such comment.)
//
// One route takes a dynamic segment, spelled `:id` here and interpolated on the other side. Between
// the mount and that segment, no text search can pair these two files.
//
// Nothing here imports the frontend and nothing here is generated from a shared schema. The only
// thing tying this file to the other tree is a contract neither repository's type-checker can see.
import express, { Router } from 'express';

const app = express();
const router = Router();
const handler = (_req: unknown, _res: unknown) => {};

router.get('/profile', handler);
router.put('/profile', handler);
router.get('/users/:id', handler);
router.post('/session', handler);

app.use('/api', router);

export default app;
