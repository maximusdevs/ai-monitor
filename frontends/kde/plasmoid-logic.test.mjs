// Table tests for the plasmoid's pure layer, in the same bare style as
// frontends/gnome/marker-logic.test.mjs: node:assert/strict, no framework, no
// dependency. Run with `node frontends/kde/plasmoid-logic.test.mjs`.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import {
    buildAccountRemoveCommand,
    buildAccountSwitchCommand, buildArgv, buildCommand, buildMonitorTestCommand, buildSessionCommand,
    buildProviderToggleCommand, buildProvidersListCommand, buildTuiCommand, cardFor,
    cardModel, cardState, DEFAULT_BINARY, DEFAULT_TIMEOUT_SECS,
    detailRows, entryFor, errorMessage, EXIT_KILLED, EXIT_TIMED_OUT, filterActiveRenewals, groupActiveRenewals,
    formatAccount, formatDuration, formatRenewalSummary, headline, isAlarming, MAX_TIMEOUT_SECS, MIN_TIMEOUT_SECS, renewalWindowDescription,
    metricDetail, nextVendor, paletteFromTheme, panelCells, parseProviders, parseReport,
    resetRemainingMs, safeText, severityColor, severityOf, SEVERITIES, shellQuote,
    shortLabel, shouldStartFetch, TIMEOUT_KILL_GRACE_SECS, timeoutSeconds,
    updatedAgeMs, vendorTabs, pruneDismissed, normalizePanel, visiblePanelItems, wrapIndex,
    inUseAccounts, buildSettingsApplyCommand, buildSettingsShowCommand,
} from './package/contents/code/plasmoid-logic.mjs';

const at = rel => fileURLToPath(new URL(rel, import.meta.url));

// ---------------------------------------------------------------------------
// V4 portability. Both of these shipped as real bugs during development and
// neither is caught by Node, which accepts them happily.
//
//   catch {   → QML's V4 engine rejects the ES2019 optional catch binding with
//               a bare "Syntax error" and the whole module fails to load.
//   \p{...}   → V4 evaluates Unicode property escapes to FALSE instead of
//               throwing. Silent wrong answers, no error anywhere.
//
// Asserting on the source text keeps this in the Node-only gate, so CI catches
// it on every platform without needing Qt installed.
// ---------------------------------------------------------------------------
// Whole-line comments are dropped first, so the comments explaining these very
// rules don't trip them. Good enough for a source heuristic; the authoritative
// check is running the module under a real applet (make mjs-probe).
const codeOnly = src => src.split('\n').filter(l => !/^\s*(\/\/|\*|\/\*)/.test(l)).join('\n');
const logicSrc = codeOnly(readFileSync(at('./package/contents/code/plasmoid-logic.mjs'), 'utf8'));

assert.ok(!/\\p\{/.test(logicSrc),
    'plasmoid-logic.mjs uses a Unicode property escape (\\p{...}). QML\'s V4 engine ' +
    'evaluates those to false silently — test for "not space or punctuation" instead.');
assert.ok(!/\}\s*catch\s*\{/.test(logicSrc),
    'plasmoid-logic.mjs uses the optional catch binding (catch {). QML\'s V4 engine ' +
    'rejects it — write catch (e) { instead.');
assert.ok(!/\{\s*\.\.\.\w+/.test(logicSrc),
    'plasmoid-logic.mjs uses object spread ({ ...obj }). QML\'s V4 engine ' +
    'rejects it with "Unexpected token ..." — use Object.assign or explicit properties instead.');

// ---------------------------------------------------------------------------
// The config schema. Both of these fail SILENTLY at runtime, which is why they
// are asserted here rather than left to review:
//
//   * A double hyphen inside an XML comment is illegal. It made the whole
//     main.xml unparseable, so every Plasmoid.configuration.* read came back
//     undefined — with no error anywhere except one "Unable to assign
//     [undefined] to QString" line.
//   * Plasma copies each config value onto a `cfg_<key>` property on the config
//     page root. A key with no matching alias simply never persists.
// ---------------------------------------------------------------------------
const configXml = readFileSync(at('./package/contents/config/main.xml'), 'utf8');
for (let i = configXml.indexOf('<!--'); i !== -1; i = configXml.indexOf('<!--', i + 4)) {
    const end = configXml.indexOf('-->', i + 4);
    assert.notEqual(end, -1, 'unterminated XML comment in config/main.xml');
    assert.ok(!configXml.slice(i + 4, end).includes('--'),
        `config/main.xml has a double hyphen inside the XML comment at offset ${i}. ` +
        `That is illegal and makes the ENTIRE schema unparseable, so every config ` +
        `default silently becomes undefined.`);
}

const configUi = readFileSync(at('./package/contents/ui/configGeneral.qml'), 'utf8');
const mainQml = readFileSync(at('./package/contents/ui/main.qml'), 'utf8');
const configAccountsUi = readFileSync(at('./package/contents/ui/configAccounts.qml'), 'utf8');
const entryNames = [...configXml.matchAll(/<entry\s+name="([^"]+)"/g)].map(m => m[1]);
assert.ok(entryNames.length >= 9, `expected the full schema, found ${entryNames.length} entries`);
const cfgDecl = name => new RegExp(`property\\s+(?:alias|\\w+)\\s+cfg_${name}\\b`);
for (const name of entryNames) {
    const pages = [configUi, configAccountsUi].filter(src => cfgDecl(name).test(src));
    assert.ok(pages.length > 0,
        `config/main.xml declares "${name}" but no config page has cfg_${name}. ` +
        `Plasma would silently never persist it.`);
    // Each page saves its own copy on Apply, so a setting shown on two pages
    // lets the stale one overwrite the edit made on the other.
    assert.ok(pages.length === 1 || ['binaryPath', 'commandTimeout'].includes(name),
        `cfg_${name} is declared on both config pages; keep one owner`);
}
// Read-only copies on the Accounts page must stay plain properties: a control
// bound to them would make the Accounts page a second editor of the setting.
for (const name of ['binaryPath', 'commandTimeout'])
    assert.ok(!new RegExp(`property\\s+alias\\s+cfg_${name}\\b`).test(configAccountsUi),
        `configAccounts.qml must not edit cfg_${name}; it is owned by configGeneral.qml`);
for (const removed of ['showSession', 'showWeekly', 'showExtra',
    'panelPools', 'panelAutoThreshold', 'showBars', 'showPercent', 'showIcon', 'showName']) {
    assert.ok(!entryNames.includes(removed),
        `${removed} was a no-op setting and must not return to the schema`);
}

// All Label/Heading text sinks opt out of AutoText. Report strings may include
// provider-controlled text; AutoText can treat an <img> tag as rich text and
// fetch its source when the popup opens.
for (const rel of [
    './package/contents/ui/ColorSwatch.qml',
    './package/contents/ui/CompactRepresentation.qml',
    './package/contents/ui/FullRepresentation.qml',
    './package/contents/ui/UsageRow.qml',
    './package/contents/ui/UsageRows.qml',
    './package/contents/ui/VendorCards.qml',
    './package/contents/ui/configGeneral.qml',
    './package/contents/ui/configAccounts.qml',
]) {
    const src = readFileSync(at(rel), 'utf8');
    const labels = (src.match(/(?:Kirigami\.Heading|PlasmaComponents\.Label|QQC2\.Label)\s*\{/g) || []).length;
    const plain = (src.match(/textFormat:\s*Text\.PlainText/g) || []).length;
    assert.equal(plain, labels, `${rel} must make every Label/Heading plain text`);
}
assert.match(configUi, /prober\.connectSource\(Logic\.buildCommand\(/,
    'the settings probe must use the same bounded, shell-quoted command builder');
assert.match(configUi, /text:\s*page\.labelFor\(modelData\)/,
    'the current-vendor delegate must label its string model through labelFor');
assert.match(mainQml, /sourceName\s*!==\s*root\.pendingCommand/,
    'a completed command must be matched to the exact in-flight command');
assert.match(mainQml, /Logic\.panelCells\(root\.entry,\s*\{/,
    'the compact view invokes panelCells with options');

// The Vendors page was removed when the report started carrying per-vendor
// status; config.qml must not still point at the deleted file, which Plasma
// reports only as an empty settings category.
const configModel = readFileSync(at('./package/contents/config/config.qml'), 'utf8');
for (const src of [...configModel.matchAll(/source:\s*"([^"]+)"/g)].map(m => m[1]))
    assert.doesNotThrow(() => readFileSync(at(`./package/contents/ui/${src}`)),
        `config.qml points at contents/ui/${src}, which does not exist`);

// ---------------------------------------------------------------------------
// A report shaped like the real one. Kept inline so the suite stays hermetic —
// it must never shell out to the binary or read a real cache.
// ---------------------------------------------------------------------------
const RAW = JSON.stringify({
    primary: 'openai',
    entries: [
        {
            id: 'anthropic', display_name: 'Claude', name: 'anthropic', plan: 'Max 20x',
            status: 'ready', stale: false, error: null,
            fetched_at: '2026-01-01T00:00:00Z',
            sections: [
                {type: 'spacer'},
                {type: 'metric', label: 'Session (5h)', value: '62%', percent: 62,
                    severity: 'mid', reset_at: '2026-01-01T02:00:00Z',
                    detail: 'Resets in 2h · 40% elapsed · 22pts over'},
                {type: 'spacer'},
                {type: 'metric', label: 'Weekly (7d)', value: '91%', percent: 91,
                    severity: 'critical', reset_at: '2026-01-04T00:00:00Z',
                    detail: 'Resets in 3d'},
            ],
        },
        {
            id: 'openai', display_name: 'Codex', name: 'openai', plan: 'Plus',
            status: 'ready', stale: true, error: null,
            fetched_at: '2026-01-01T00:00:00Z',
            sections: [{type: 'block', label: 'Credits', body: ['balance: $4.10']}],
        },
        {
            id: 'zai', display_name: 'Z.AI', name: 'zai', plan: '',
            status: 'error', stale: false, error: 'no API key', sections: [],
        },
    ],
});
const report = parseReport(RAW);
const NOW = Date.parse('2026-01-01T00:30:00Z');

assert.equal(report.ok, true);
assert.equal(report.entries.length, 3);
assert.equal(report.primary, 'openai');

// The binary always exits 0 and prints a report, so anything unparseable is a
// missing or broken binary — never a vendor-side failure.
for (const bad of ['', '   ', 'not json', '[]', '{}', 'null', '{"entries":null}'])
    assert.equal(parseReport(bad).ok, false, `must reject ${JSON.stringify(bad)}`);
assert.equal(parseReport('not json').raw, 'not json', 'the original output is kept for the error line');
// An entry with no id cannot be selected or tabbed to, so it is dropped rather
// than rendered as a nameless row.
assert.equal(parseReport(JSON.stringify({entries: [{plan: 'x'}]})).entries.length, 0);

// Number(null) === 0 in JS. A metric the report sends with percent null (a
// balance-style row, or a window the vendor does not report) must normalize to
// null so the views can show "not reported" — not to a fabricated 0% bar.
const nullPercent = parseReport(JSON.stringify({entries: [{id: 'v', sections: [
    {type: 'metric', label: 'Balance', value: '$4.10', percent: null},
]}]})).entries[0].sections[0];
assert.equal(nullPercent.percent, null);
assert.equal(nullPercent.severity, 'low');

// Provider-controlled strings cannot turn into QML rich text, preserve terminal
// controls, or use bidi overrides to disguise what the panel displays.
const hostile = parseReport(JSON.stringify({entries: [{
    id: 'hostile', display_name: '<img src="https://example.invalid/pixel">\u202eevil',
    plan: '<b>plan</b>', status: 'error', error: '\u001b[31mboom',
    sections: [{type: 'block', label: '<i>credits</i>', body: ['<img src="x">']}],
}]}));
const hostileText = JSON.stringify(hostile.entries[0]);
assert.ok(!/[<>\u001b\u202e]/.test(hostileText),
    'report normalization must remove rich-text delimiters and display controls');
assert.match(hostile.entries[0].label, /‹img src=/,
    'hostile markup is shown as inert text rather than silently discarded');

const oversized = parseReport(JSON.stringify({entries: Array.from({length: 80}, (_, i) => ({
    id: `v${i}`, sections: Array.from({length: 160}, () => ({
        type: 'block', body: Array.from({length: 160}, () => 'x'),
    })),
}))}));
assert.equal(oversized.entries.length, 64, 'entry rendering is bounded');
assert.equal(oversized.entries[0].sections.length, 128, 'section rendering is bounded');
assert.equal(oversized.entries[0].sections[0].body.length, 128, 'block rendering is bounded');

// ---------------------------------------------------------------------------
// severity
// ---------------------------------------------------------------------------
for (const s of SEVERITIES)
    assert.equal(severityOf(0, s), s, 'a declared severity always wins');
// Falls back to the documented 50/75/90 bands when the field is missing or
// unrecognised, so an older binary still colours sensibly.
assert.equal(severityOf(0, ''), 'low');
assert.equal(severityOf(49, undefined), 'low');
assert.equal(severityOf(50, 'nonsense'), 'mid');
assert.equal(severityOf(74, null), 'mid');
assert.equal(severityOf(75, ''), 'high');
assert.equal(severityOf(89, ''), 'high');
assert.equal(severityOf(90, ''), 'critical');
assert.equal(severityOf(null, ''), 'low', 'no percentage is not a crisis');

const colors = {low: 'L', mid: 'M', high: 'H', critical: 'C', empty: 'E'};
assert.equal(severityColor('low', colors), 'L');
assert.equal(severityColor('mid', colors), 'M');
assert.equal(severityColor('high', colors), 'H');
assert.equal(severityColor('critical', colors), 'C');
assert.equal(severityColor('nonsense', colors), 'L', 'an unknown severity must not be blank');
assert.equal(severityColor('low', undefined), undefined);

// ---------------------------------------------------------------------------
// entry selection
// ---------------------------------------------------------------------------
assert.equal(entryFor(report, 'zai').id, 'zai');
// A vendor dropped from config.toml must degrade to something visible, never a
// blank panel: the report's own primary, then the first entry.
assert.equal(entryFor(report, 'deepseek').id, 'openai', 'falls back to the report primary');
assert.equal(entryFor({entries: report.entries, primary: ''}, 'nope').id, 'anthropic');
assert.equal(entryFor(null, 'anthropic'), null);
assert.equal(entryFor({entries: []}, 'anthropic'), null);

// display_name is the canonical label; the raw id shows only if it is missing.
assert.deepEqual(vendorTabs(report, 'anthropic').map(t => t.label), ['Claude', 'Codex', 'Z.AI']);
assert.deepEqual(vendorTabs(report, 'anthropic').map(t => t.active), [true, false, false]);
// Every configured vendor is offered, including failing ones — hiding a broken
// vendor made "not configured" indistinguishable from "configured and broken".
assert.deepEqual(vendorTabs(report, 'anthropic').map(t => t.failing), [false, false, true]);
assert.equal(vendorTabs(report, 'gone').filter(t => t.active).length, 1,
    'an unknown id must still leave exactly one tab active');

// ---------------------------------------------------------------------------
// projection
// ---------------------------------------------------------------------------
const anthropic = entryFor(report, 'anthropic');
const openai = entryFor(report, 'openai');
const zai = entryFor(report, 'zai');

assert.equal(anthropic.label, 'Claude');
assert.equal(anthropic.plan, 'Max 20x');
assert.equal(openai.stale, true);
assert.equal(zai.status, 'error');

// The headline is the WORST window, not the first.
assert.equal(headline(anthropic).text, '91%');
assert.equal(headline(anthropic).severity, 'critical');
assert.equal(headline(anthropic).label, 'Weekly (7d)');
assert.equal(headline(zai).text, 'Error');
assert.equal(headline(null).text, '');

assert.equal(isAlarming(anthropic), true, 'a critical window is alarming');
assert.equal(isAlarming(openai), true, 'so is stale data');
assert.equal(isAlarming(zai), true, 'so is an errored vendor');
assert.equal(isAlarming(null), false);

// Spacers are dropped: Column spacing sets the rhythm, so keeping them would
// double it.
assert.equal(detailRows(anthropic).length, 2);
assert.deepEqual(detailRows(anthropic).map(r => r.type), ['metric', 'metric']);
assert.deepEqual(detailRows(openai).map(r => r.type), ['block']);
assert.deepEqual(detailRows(null), []);
// A block section keeps its free-form lines rather than being flattened away.
assert.deepEqual(detailRows(openai)[0].body, ['balance: $4.10']);

// Raw JSON errors and auth/logout text errors are suppressed ("apenas n mostre nada")
const withErr = {
    sections: [
        { type: 'metric', label: 'Gemini', percent: 96, value: '96%', severity: 'critical' },
        { type: 'text', label: 'HTTP 500', value: '{"code":"internal","message":"You are not logged into Antigravity"}' },
    ],
};
assert.equal(detailRows(withErr).length, 1);
assert.equal(detailRows(withErr)[0].label, 'Gemini');

assert.deepEqual(panelCells(anthropic, {max: 2}).map(c => c.text), ['62%', '91%']);
assert.deepEqual(panelCells(anthropic, {max: 2}).map(c => c.label), ['Claude', '7d']);
assert.deepEqual(panelCells(anthropic, {max: 1}).map(c => c.text), ['62%']);
assert.equal(panelCells(anthropic, {max: 2})[1].severity, 'critical');
// An errored vendor renders the same ⚠ the GNOME and macOS panels use, never a
// confident 0%.
assert.deepEqual(panelCells(zai).map(c => c.text), ['⚠']);
assert.deepEqual(panelCells(null), []);
// A vendor whose only section is a block has no percentage to plot.
assert.deepEqual(panelCells(openai), []);

const agyEntry = {
    id: 'antigravity',
    sections: [
        { type: 'metric', label: 'Gemini', percent: 100, severity: 'critical' },
        { type: 'metric', label: 'Claude & GPT OSS', percent: 5, severity: 'low' },
        { type: 'metric', label: 'Gemini', percent: 17, severity: 'low' },
        { type: 'metric', label: 'Claude & GPT OSS', percent: 35, severity: 'low' },
    ]
};
assert.deepEqual(panelCells(agyEntry, {max: 2, showExtraModels: true}).map(c => c.label), ['Gemini', 'Claude']);
assert.equal(panelCells(agyEntry, {max: 2, showExtraModels: true})[1].isExtra, true);
assert.deepEqual(panelCells(agyEntry, {max: 1, showExtraModels: false}).map(c => c.label), ['Gemini']);

// Show 5h and Weekly together, with and without extra models
assert.deepEqual(panelCells(agyEntry, {showWeekly: true, showExtraModels: true}).map(c => c.label), ['Gemini', '7d', 'Claude']);
assert.equal(panelCells(agyEntry, {showWeekly: true, showExtraModels: true}).length, 3, 'When both 5h + week and extra models are enabled, it must show all 3');
assert.equal(panelCells(agyEntry, {showWeekly: true, showExtraModels: true})[2].isExtra, true);
assert.deepEqual(panelCells(agyEntry, {showWeekly: true, showExtraModels: false}).map(c => c.label), ['Gemini', '7d']);
assert.equal(panelCells(agyEntry, {showWeekly: true, showExtraModels: false}).length, 2, 'When both 5h + week are enabled without extra models, it must show 2');

// Show only 5h
assert.deepEqual(panelCells(agyEntry, {showWeekly: false, showExtraModels: true}).map(c => c.label), ['Gemini', 'Claude']);
assert.deepEqual(panelCells(agyEntry, {showWeekly: false, showExtraModels: false}).map(c => c.label), ['Gemini']);

// Generic provider (anthropic) showWeekly toggles
assert.deepEqual(panelCells(anthropic, {showWeekly: true}).map(c => c.label), ['Claude', '7d']);
assert.deepEqual(panelCells(anthropic, {showWeekly: false}).map(c => c.label), ['Claude']);
// The account view hands over `name` rather than a normalized `label`.
assert.deepEqual(
    panelCells({id: 'anthropic', name: 'Claude', metrics: [
        {label: 'Session (5h)', percent: 6, windowSecs: 18000},
        {label: 'Weekly (7d)', percent: 1, windowSecs: 604800},
    ]}).map(c => `${c.label} ${c.text}`),
    ['Claude 6%', '7d 1%'],
);
// A metric that already names itself keeps its own word.
assert.deepEqual(
    panelCells({id: 'openai', name: 'Codex', metrics: [{label: 'Codex 5h', percent: 18, windowSecs: 18000}]}).map(c => c.label),
    ['Codex'],
);

// A monthly metric may arrive before the weekly one. The compact panel must
// still reserve the second slot for the configured 7-day quota.
const providerWithMonthlyBeforeWeekly = {
    id: 'provider-with-pools',
    sections: [
        { type: 'metric', label: 'Session (5h)', percent: 12, windowSecs: 18000 },
        { type: 'metric', label: 'Monthly (30d)', percent: 30, windowSecs: 2592000 },
        { type: 'metric', label: 'Weekly', percent: 70, windowSecs: 604800 },
    ]
};
assert.deepEqual(
    panelCells(providerWithMonthlyBeforeWeekly, {showWeekly: true, max: 2}).map(c => c.label),
    ['5h', '7d'],
);
assert.deepEqual(
    panelCells(providerWithMonthlyBeforeWeekly, {showWeekly: true, max: 2}).map(c => c.text),
    ['12%', '70%'],
);

assert.equal(detailRows(agyEntry, true).length, 4);
assert.equal(detailRows(agyEntry, false).length, 2);

// Verify OpenAI / Codex is NEVER treated as an extra model or filtered out
const codexEntry = {
    id: 'openai',
    sections: [
        { type: 'metric', label: 'Codex 5h', percent: 20, severity: 'low' },
        { type: 'metric', label: 'GPT-4o', percent: 50, severity: 'mid' }
    ]
};
assert.equal(panelCells(codexEntry, {max: 2, showExtraModels: false})[0].isExtra, false);
assert.equal(panelCells(codexEntry, {max: 2, showExtraModels: false})[1].isExtra, false);
assert.deepEqual(panelCells(codexEntry, {max: 2, showExtraModels: false}).map(c => c.label), ['Codex', 'GPT-4o']);
assert.equal(detailRows(codexEntry, false).length, 2, 'OpenAI metrics with GPT in label must NOT be filtered out by showExtraModels');
assert.equal(detailRows(codexEntry, true).length, 2);

// ---------------------------------------------------------------------------
// cards (viewMode "VendorCards")
// ---------------------------------------------------------------------------
const claudeCard = cardFor(anthropic);
const codexCard = cardFor(openai);
const zaiCard = cardFor(zai);

// One card per entry the aggregate report returned, in report order — the
// card view never restates a provider list of its own.
assert.deepEqual(cardModel(report).map(c => c.label), ['Claude', 'Codex', 'Z.AI']);
assert.equal(cardModel(null).length, 0);
assert.equal(cardFor(null), null);
assert.equal(cardState(null), 'ok');

// Windows are the metric sections, in report order; block sections ride along.
assert.deepEqual(claudeCard.windows.map(w => w.window), ['5h', '7d']);
assert.deepEqual(claudeCard.windows.map(w => w.value), ['62%', '91%']);
assert.deepEqual(claudeCard.windows.map(w => w.severity), ['mid', 'critical']);
assert.equal(claudeCard.windows[0].detail, '40% elapsed · 22pts over');
assert.equal(claudeCard.blocks.length, 0);
assert.equal(codexCard.blocks.length, 1);

// The accent is the WORST window — the same rule headline() applies.
assert.equal(claudeCard.accent, 'critical');

// Staleness is the report's own verdict (the Rust core owns it), and a stale
// card keeps the severity of its last good numbers instead of flipping red.
assert.equal(claudeCard.state, 'ok');
assert.equal(codexCard.state, 'stale');
assert.equal(codexCard.accent, 'low');
assert.deepEqual(codexCard.windows, []);

// An errored vendor outranks whatever its last good numbers said, replaces the
// gauges with the message, and never renders as a 0% bar.
assert.equal(zaiCard.state, 'error');
assert.equal(zaiCard.accent, 'critical');
assert.equal(zaiCard.error, 'no API key');
assert.deepEqual(zaiCard.windows, []);

// A window a vendor does not report carries percent null and its value text —
// the QML renders that as "not reported", never as a fabricated 0%.
const partial = cardFor(parseReport(JSON.stringify({entries: [{
    id: 'openrouter', display_name: 'OpenRouter', name: 'openrouter', plan: '',
    status: 'ready', stale: false, error: null,
    sections: [{type: 'metric', label: 'Session (5h)', value: '$1.20',
        percent: null, severity: 'low'}],
}]})).entries[0]);
assert.deepEqual(partial.windows.map(w => [w.percent, w.value]), [[null, '$1.20']]);
assert.equal(partial.accent, 'low');

assert.equal(shortLabel('Session (5h)'), '5h');
assert.equal(shortLabel('Weekly (7d)'), '7d');
// The parenthetical is the window descriptor, which is exactly the tag the
// panel wants — even when it is a word rather than a duration.
assert.equal(shortLabel('MCP tools (monthly)'), 'monthly');
// Nothing parenthesised, or something too long to be a window, falls back to
// the first word rather than to an empty tag.
assert.equal(shortLabel('Credits'), 'Credits');
assert.equal(shortLabel('Balance (since last invoice)'), 'Balance');
assert.equal(shortLabel(''), '');
assert.equal(shortLabel(undefined), '');

// detail still carries a "Resets in …" written for CLI readers; the popup
// renders a live countdown instead, so that fragment would duplicate and go
// stale.
assert.equal(metricDetail(detailRows(anthropic)[0]), '40% elapsed · 22pts over');
assert.equal(metricDetail(detailRows(anthropic)[1]), '', 'a reset-only detail collapses to nothing');
assert.equal(metricDetail(null), '');
assert.equal(metricDetail({detail: 'kept', resetAt: ''}), 'kept');

// ---------------------------------------------------------------------------
// time
// ---------------------------------------------------------------------------
assert.equal(formatDuration(0), 'now');
assert.equal(formatDuration(-1), 'now');
assert.equal(formatDuration(30 * 1000), '1m', 'under a minute still reads as 1m, never 0m');
assert.equal(formatDuration(62 * 60 * 1000), '1h 2m');
assert.equal(formatDuration(26 * 3600 * 1000), '1d 2h');
assert.equal(formatDuration('nonsense'), 'now');

assert.equal(resetRemainingMs('2026-01-01T02:00:00Z', NOW), 90 * 60 * 1000);
assert.ok(resetRemainingMs('2026-01-01T00:00:00Z', NOW) < 0, 'a past reset is negative, not clamped');
assert.equal(resetRemainingMs('', NOW), null);
assert.equal(resetRemainingMs('not a date', NOW), null);
assert.equal(resetRemainingMs(undefined, NOW), null);

assert.equal(updatedAgeMs('2026-01-01T00:00:00Z', NOW), 30 * 60 * 1000);
assert.equal(updatedAgeMs('2026-01-01T01:00:00Z', NOW), 0, 'a clock skew must not read as negative age');
assert.equal(updatedAgeMs('', NOW), null);
assert.equal(updatedAgeMs('not a date', NOW), null);

assert.equal(errorMessage(''), 'The usage command failed without an error message.');
assert.equal(errorMessage('  boom  '), 'boom');
assert.equal(safeText('<b>x</b>\u202e'), '‹b›x‹/b›');

// ---------------------------------------------------------------------------
// the scroll ring
// ---------------------------------------------------------------------------
const ring = ['anthropic', 'openai', 'zai'];
assert.equal(nextVendor(ring, 'anthropic', 1), 'openai');
assert.equal(nextVendor(ring, 'zai', 1), 'anthropic', 'wraps forward');
assert.equal(nextVendor(ring, 'anthropic', -1), 'zai', 'wraps backward');
assert.equal(nextVendor(ring, 'deepseek', 1), 'anthropic', 'a vendor dropped from config has no neighbour');
assert.equal(nextVendor([], 'anthropic', 1), 'anthropic', 'empty ring is a no-op');
assert.equal(nextVendor(null, 'anthropic', 1), 'anthropic');
assert.equal(nextVendor(ring, 'anthropic', 4), 'openai', 'a fast flick accumulates steps');

// A KConfigXT StringList reaches QML as an array-LIKE object: right length,
// right contents, but Array.isArray() === false. Guarding on isArray made every
// scroll a silent no-op in the real panel while every test here still passed.
const arrayLike = {0: 'anthropic', 1: 'openai', 2: 'zai', length: 3};
assert.equal(Array.isArray(arrayLike), false, 'the fixture must not be a real Array');
assert.equal(nextVendor(arrayLike, 'anthropic', 1), 'openai',
    'an array-like ring (what KConfig actually hands QML) must still cycle');
assert.equal(nextVendor(arrayLike, 'zai', 1), 'anthropic');

// ---------------------------------------------------------------------------
// the command
// ---------------------------------------------------------------------------
assert.equal(shellQuote('plain'), `'plain'`);
assert.equal(shellQuote(`it's`), `'it'\\''s'`, 'embedded quote is escaped, not dropped');
assert.equal(shellQuote(''), `''`);
assert.equal(shellQuote(null), `''`);

// One call covers every vendor, so --vendor never appears: the applet picks its
// entry client side and never reads the shared active_vendor file, which
// belongs to the Waybar module's --cycle-next.
for (const args of [buildArgv('', 0), buildArgv('b', 60)])
    assert.equal(args.indexOf('--vendor'), -1,
        'the plasmoid must never pass --vendor: that path reads the shared ' +
        'active_vendor file and two instances would collide');

// The timeout(1) wrapper. The data engine gives QML no way to kill a hung
// child, so this is the only thing that actually bounds it.
assert.deepEqual(buildArgv('b', 60),
    ['timeout', '-k', String(TIMEOUT_KILL_GRACE_SECS), '60', 'b', 'usage', '--json']);
assert.equal(timeoutSeconds(null), DEFAULT_TIMEOUT_SECS);
assert.equal(timeoutSeconds(undefined), DEFAULT_TIMEOUT_SECS);
assert.equal(timeoutSeconds('soon'), DEFAULT_TIMEOUT_SECS);
assert.equal(timeoutSeconds(0), MIN_TIMEOUT_SECS);
assert.equal(timeoutSeconds(45.6), MIN_TIMEOUT_SECS);
assert.equal(timeoutSeconds(600), 600);
assert.equal(timeoutSeconds(9999), MAX_TIMEOUT_SECS);
for (const bad of [0, -1, null, undefined, NaN, 'soon', Infinity]) {
    const args = buildArgv('b', bad);
    assert.equal(args[0], 'timeout', `timeout remains mandatory for ${String(bad)}`);
    assert.deepEqual(args.slice(-3), ['b', 'usage', '--json']);
}
assert.equal(EXIT_TIMED_OUT, 124);
assert.equal(EXIT_KILLED, 137);

// KShell::splitArgs(AbortOnMeta) refuses the whole string on an unquoted
// metacharacter, so every argument has to come back single-quoted.
const cmd = buildCommand('/opt/my apps/ai-monitor', 60);
assert.ok(cmd.startsWith(`'timeout' '-k' '5' '60' `), 'the wrapper must lead the command');
assert.ok(cmd.includes(`'/opt/my apps/ai-monitor'`), 'a path with spaces stays one argument');
assert.ok(!/[;{}]/.test(cmd.replace(/'[^']*'/g, '')),
    'no shell metacharacter may appear outside a quoted span');

// The TUI launcher is the opposite case: the metacharacters ARE the point,
// because there is no portable way to probe PATH from QML.
assert.ok(buildTuiCommand('').includes('||'), 'the fallback chain needs its ||');
assert.equal(buildTuiCommand('kitty -e'), `kitty -e 'ai-monitor-tui'`);
assert.equal(buildTuiCommand('  '), buildTuiCommand(''), 'blank means "no custom terminal"');

assert.equal(shouldStartFetch('', 'cmd'), true);
assert.equal(shouldStartFetch('cmd', 'cmd'), false, 'never queue a second identical fetch');
assert.equal(shouldStartFetch('old', 'new'), false,
    'a config change must not start a second command while one is in flight');
assert.equal(shouldStartFetch('', ''), false, 'an empty command is never executable');

// ---------------------------------------------------------------------------
// theme
// ---------------------------------------------------------------------------
const palette = paletteFromTheme({
    textColor: 'T', neutralTextColor: 'N', negativeTextColor: 'G',
    positiveTextColor: 'P', disabledTextColor: 'D',
});
assert.equal(palette.low, 'P');
assert.equal(palette.critical, 'G');
assert.equal(palette.empty, 'D');
// Every role must resolve to something, or a theme missing one role paints an
// invisible bar.
// ---------------------------------------------------------------------------
// options, account formatting, provider management, extra models
// ---------------------------------------------------------------------------
assert.deepEqual(buildArgv('b', 60, {refresh: true}),
    ['timeout', '-k', String(TIMEOUT_KILL_GRACE_SECS), '60', 'b', 'usage', '--json', '--refresh']);
assert.deepEqual(buildArgv('b', 60, {account: 'maximus'}),
    ['timeout', '-k', String(TIMEOUT_KILL_GRACE_SECS), '60', 'b', 'usage', '--json', '--account', 'maximus']);
assert.deepEqual(buildArgv('b', 60, {refresh: true, account: 'maximus'}),
    ['timeout', '-k', String(TIMEOUT_KILL_GRACE_SECS), '60', 'b', 'usage', '--json', '--refresh', '--account', 'maximus']);
assert.deepEqual(buildArgv('b', 60, {refresh: true, provider: 'antigravity'}),
    ['timeout', '-k', String(TIMEOUT_KILL_GRACE_SECS), '60', 'b', 'usage', '--json', '--refresh', '--provider', 'antigravity']);
assert.deepEqual(buildArgv('b', 60, {refresh: true, allProviders: true}),
    ['timeout', '-k', String(TIMEOUT_KILL_GRACE_SECS), '60', 'b', 'usage', '--json', '--refresh', '--all-providers']);

assert.equal(formatAccount('user@example.com', true), 'user@example.com');
assert.equal(formatAccount('user@example.com', false), 'user');
assert.equal(formatAccount('maximus', false), 'maximus');
assert.equal(formatAccount('', false), '');

assert.equal(buildAccountSwitchCommand('ai-monitor', 'maximus'), `'ai-monitor' 'account' 'switch' 'maximus'`);
assert.equal(buildAccountRemoveCommand('ai-monitor', 'maximus'), `'ai-monitor' 'account' 'remove' 'maximus'`);
assert.equal(buildProviderToggleCommand('ai-monitor', 'antigravity', true), `'ai-monitor' 'provider' 'enable' 'antigravity'`);
assert.equal(buildProviderToggleCommand('ai-monitor', 'antigravity', false), `'ai-monitor' 'provider' 'disable' 'antigravity'`);
assert.equal(buildProvidersListCommand('ai-monitor'), `'ai-monitor' 'provider' 'list' '--json'`);
assert.equal(buildMonitorTestCommand('ai-monitor'), `'ai-monitor' 'monitor' '--test'`);
assert.equal(buildSessionCommand('ai-monitor', 60),
    `'timeout' '-k' '5' '60' 'ai-monitor' 'sessions' '--json'`);

const provList = parseProviders(JSON.stringify([
    {id: 'agy', name: 'Antigravity', enabled: true, configured: true},
    {id: 'claude', name: 'Claude', enabled: false, configured: false}
]));
assert.equal(provList.length, 2);
assert.equal(provList[0].id, 'agy');
assert.equal(provList[0].enabled, true);
assert.equal(provList[1].enabled, false);

const reportWithAccAndModels = parseReport(JSON.stringify({
    primary: 'antigravity',
    account: {
        label: 'user@example.com',
        user: 'user@example.com',
        providers: ['antigravity']
    },
    accounts: [
        {label: 'user@example.com', user: 'user@example.com', active: true},
        {label: 's2.luan2009@gmail.com', user: 's2.luan2009@gmail.com', active: false}
    ],
    entries: [
        {
            id: 'antigravity', display_name: 'Antigravity', status: 'ready',
            extra_models: ['Claude Sonnet 4.6 (Thinking)', 'Claude Opus 4.6 (Thinking)', 'GPT-OSS 120B (Medium)'],
            sections: [{type: 'metric', label: 'Session (5h)', value: '10%', percent: 10}]
        }
    ]
}));
assert.equal(reportWithAccAndModels.account.label, 'user@example.com');
assert.deepEqual(reportWithAccAndModels.account.providers, ['antigravity']);
assert.equal(reportWithAccAndModels.accounts.length, 2);
assert.equal(reportWithAccAndModels.accounts[0].active, true);
assert.equal(reportWithAccAndModels.entries[0].extraModels.length, 3);
assert.equal(reportWithAccAndModels.entries[0].extraModels[0], 'Claude Sonnet 4.6 (Thinking)');
const agyCard = cardFor(reportWithAccAndModels.entries[0]);
assert.equal(agyCard.extraModels.length, 3);

// Recent is provider-scoped: a Gemini session change must not mark a Codex
// account as recent just because that account is globally active.
const recentAccounts = parseReport(JSON.stringify({
    entries: [],
    accounts: [
        {label: 'gemini-new@example.com', providers: [{id: 'antigravity', recent: true}]},
        {label: 'codex@example.com', providers: [{id: 'openai', recent: true}]},
        {label: 'gemini-old@example.com', providers: [{id: 'antigravity', recent: false}]}
    ]
}));
assert.equal(recentAccounts.accounts[0].providers[0].recent, true);
assert.equal(recentAccounts.accounts[1].providers[0].recent, true);
assert.equal(recentAccounts.accounts[2].providers[0].recent, false);

// Renewal helpers tests
const rawRenewals = [
    {id: 'r1', account_label: 'user1@gmail.com', provider_name: 'Antigravity', metric_label: 'Claude', window_type: '5h'},
    {id: 'r2', account_label: 'user2@gmail.com', provider_name: 'Codex', metric_label: 'Codex 5h', window_type: '5h'}
];
assert.equal(filterActiveRenewals(rawRenewals, []).length, 2);
assert.equal(filterActiveRenewals(rawRenewals, ['r1']).length, 1);
assert.equal(filterActiveRenewals(rawRenewals, ['r1'])[0].id, 'r2');
assert.equal(filterActiveRenewals(rawRenewals, ['r1', 'r2']).length, 0);
assert.equal(renewalWindowDescription(rawRenewals[0]), 'modelo extra — janela de 5h');
assert.equal(renewalWindowDescription({provider_id: 'antigravity', metric_label: 'Gemini', window_type: 'Semanal'}), 'modelo normal — janela semanal');
assert.equal(formatRenewalSummary(rawRenewals[0], false), 'user1 [Antigravity]: Claude (modelo extra — janela de 5h)');
assert.equal(formatRenewalSummary(rawRenewals[0], true), 'user1@gmail.com [Antigravity]: Claude (modelo extra — janela de 5h)');
const groupedRenewals = groupActiveRenewals([
    rawRenewals[0],
    {id: 'r3', account_label: 'user1@gmail.com', provider_name: 'Antigravity', metric_label: 'Gemini', window_type: 'Semanal'},
    rawRenewals[1]
]);
assert.equal(groupedRenewals.length, 3);
assert.equal(groupedRenewals[0].renewals.length, 1);

// Panel: order and eligibility come from the binary; the applet only picks
// which of the items are on screen.
const panel = normalizePanel({
    mode: 'expanded', unit: 'account', count: 2, interval: 7, hover: 'pager',
    items: [
        {key: 'a@x', title: 'a', active: true, providers: [{id: 'openai', name: 'Codex', metrics: [
            {label: 'Codex 5h', percent: 40, value: '40%', severity: 'low', window_secs: 18000,
             reset_at: '2026-09-26T12:00:00Z'}]}]},
        {key: 'b@x', title: 'b', providers: []},
        {key: 'c@x', title: 'c', providers: []},
        {title: 'no key is dropped'},
    ],
});
assert.equal(panel.items.length, 3);
assert.equal(panel.items[0].providers[0].metrics[0].windowSecs, 18000);
assert.equal(panel.items[0].providers[0].metrics[0].resetAt, '2026-09-26T12:00:00Z');
assert.deepEqual(visiblePanelItems(panel, 0).map(i => i.key), ['a@x', 'b@x']);
const carousel = Object.assign({}, panel, {mode: 'carousel'});
assert.deepEqual(visiblePanelItems(carousel, 4).map(i => i.key), ['b@x']);
assert.deepEqual(visiblePanelItems(carousel, -1).map(i => i.key), ['c@x']);
assert.deepEqual(visiblePanelItems(normalizePanel(null), 0), []);
assert.equal(wrapIndex(5, 0), 0);
const odd = normalizePanel({mode: 'sideways', count: 99, interval: 0, hover: 7, items: 'x'});
assert.deepEqual([odd.mode, odd.count, odd.interval, odd.hover, odd.items.length],
    ['expanded', 6, 2, 'blocks', 0], 'malformed values fall back, never reach the QML');
assert.equal(parseReport('{"entries":[]}').panel.items.length, 0, 'an older binary has no panel');
assert.equal(panel.showAccountName, true, 'account names default on');
assert.equal(normalizePanel({show_account_name: false}).showAccountName, false);

// The click popup lists the accounts in use now.
const used = inUseAccounts([
    {label: 'a', active: true, providers: [{id: 'openai', recent: false}]},
    {label: 'b', active: false, providers: [{id: 'openai', recent: true}, {id: 'anthropic', recent: false}]},
]);
assert.deepEqual(used.map(a => a.label), ['b']);
assert.deepEqual(used[0].providers.map(p => p.id), ['openai']);
assert.deepEqual(inUseAccounts([{label: 'a', active: true, providers: []}, {label: 'b', providers: []}])
    .map(a => a.label), ['a'], 'no recency: the active account');

// settings apply: the pipe must be inside a quoted script, and the JSON must
// reach stdin byte for byte even with quotes in it.
assert.equal(buildSettingsShowCommand(''), "'ai-monitor' 'settings' 'show'");
{
    const {execSync} = await import('node:child_process');
    const {mkdtempSync, writeFileSync, chmodSync} = await import('node:fs');
    const {tmpdir} = await import('node:os');
    const {join} = await import('node:path');
    const dir = mkdtempSync(join(tmpdir(), 'plasmoid-'));
    const stub = join(dir, 'fake bin');
    writeFileSync(stub, '#!/bin/sh\n[ "$1 $2" = "settings apply" ] || exit 9\ncat\n');
    chmodSync(stub, 0o755);
    const patch = {display: {hidden_accounts: ["it's \"odd\" $(x)"]}};
    const cmd = buildSettingsApplyCommand(stub, patch);
    assert.ok(!/(^|[^'])\|/.test(cmd.replace(/'[^']*'/g, '')), 'no unquoted pipe');
    const out = execSync(cmd, {encoding: 'utf8'});
    assert.deepEqual(JSON.parse(out), Object.assign({schema_version: 1}, patch));
}

assert.deepEqual(pruneDismissed(['a', 'b', 'c'], [{id: 'c'}, {id: 'd'}]), ['c'],
    'dismissed ids the report no longer carries are forgotten');
assert.deepEqual(pruneDismissed(undefined, [{id: 'c'}]), []);
assert.deepEqual(pruneDismissed(['a'], undefined), []);

console.log('plasmoid logic tests passed');
