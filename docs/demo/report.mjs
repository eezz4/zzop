// Reads one `zzop cross` reply and ASSERTS the join state the break-a-route demo claims at that step,
// then prints it. Exits non-zero on any mismatch, so the demo cannot narrate a state it is not in.
// See break-a-route-shipped.sh for why that matters here.
//
// ASSERT BEFORE NARRATE, on purpose. The first version printed its sentences first and checked after,
// so a broken run still emitted "every call is answered by a route" and only then failed. A reader
// skimming the output would have read the claim and missed the exit code -- which is the same defect
// this demo exists to prevent, one level up.
//
// Usage:  node docs/demo/report.mjs <reply.json> <baseline|drifted>
import fs from 'node:fs';

const [, , replyPath, phase] = process.argv;
if (!replyPath || !['baseline', 'drifted'].includes(phase ?? '')) {
  console.error('usage: node docs/demo/report.mjs <reply.json> <baseline|drifted>');
  process.exit(2);
}

const reply = JSON.parse(fs.readFileSync(replyPath, 'utf8'));
const b = reply.buckets ?? {};
const k = reply.distinctBucketKeys ?? {};
const list = (name) => (k[name] ?? []).slice().sort();
const edgeKeys = (reply.edges ?? []).map((e) => e.key ?? e).sort();

// What must be true for this step's sentences to be honest.
const EXPECTED = {
  baseline: {
    edges: 4,
    unprovidedConsumes: [],
    unconsumedProvides: [],
    edgeKeys: ['GET /api/profile', 'GET /api/users/{}', 'POST /api/session', 'PUT /api/profile'],
  },
  drifted: {
    edges: 3,
    unprovidedConsumes: ['PUT /api/profile'],
    unconsumedProvides: ['PUT /api/account'],
    edgeKeys: null, // the point of this step is the two buckets, not which three survived
  },
}[phase];

const problems = [];
if (b.edges !== EXPECTED.edges) problems.push(`edges: expected ${EXPECTED.edges}, got ${b.edges}`);
for (const bucket of ['unprovidedConsumes', 'unconsumedProvides']) {
  const got = list(bucket);
  if (JSON.stringify(got) !== JSON.stringify(EXPECTED[bucket]))
    problems.push(`${bucket}: expected ${JSON.stringify(EXPECTED[bucket])}, got ${JSON.stringify(got)}`);
}
if (EXPECTED.edgeKeys && JSON.stringify(edgeKeys) !== JSON.stringify(EXPECTED.edgeKeys))
  problems.push(`edge keys: expected ${JSON.stringify(EXPECTED.edgeKeys)}, got ${JSON.stringify(edgeKeys)}`);

if (problems.length) {
  console.error(`   edges ${b.edges}   unprovidedConsumes ${b.unprovidedConsumes}   unconsumedProvides ${b.unconsumedProvides}`);
  console.error('');
  console.error(`!! the ${phase} step does not hold any more:`);
  for (const p of problems) console.error(`     ${p}`);
  console.error('');
  console.error('   The demo asserts rather than narrates on purpose. Either the pair under');
  console.error('   docs/demo/pair/ changed, or the join changed -- find out which before editing');
  console.error('   these expectations, because one of the two is a regression.');
  process.exit(1);
}

console.log(`   edges                ${b.edges}`);
console.log(`   unprovidedConsumes   ${b.unprovidedConsumes}${list('unprovidedConsumes').length ? '  ' + JSON.stringify(list('unprovidedConsumes')) : ''}`);
console.log(`   unconsumedProvides   ${b.unconsumedProvides}${list('unconsumedProvides').length ? '  ' + JSON.stringify(list('unconsumedProvides')) : ''}`);

if (phase === 'baseline') {
  console.log('   -> every call the frontend makes is answered by a route the backend registers.');
  console.log(`   -> matched keys: ${JSON.stringify(edgeKeys)}`);
  console.log('      Neither of those keys exists as text on either side. `/api` came from the mount in');
  console.log('      the backend, and `{}` is `:id` and `${id}` normalised to the same segment.');
} else {
  console.log('   -> the frontend still calls PUT /api/profile, and nothing serves it any more.');
  console.log('   -> the backend now serves PUT /api/account, and nobody calls it.');
  console.log('      Both sides are named. Neither repository could have told you this on its own.');
}
