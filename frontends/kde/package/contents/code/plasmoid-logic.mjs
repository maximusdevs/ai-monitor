// Pure layer for the KDE plasmoid. No Qt imports, so every rule here is table
// tested under Node in frontends/kde/plasmoid-logic.test.mjs.
//
// Consumes `ai-monitor usage --json` — the same report the native Omarchy
// panel consumes — rather than the `--format` placeholder string. That contract
// already carries display_name, plan, status, stale, fetched_at, per-metric
// severity and an absolute reset_at, so the widget stays a presentation layer
// and needs no copy of the shared marker-logic module.
//
// Formatting semantics (headline, formatDuration, formatReset, formatUpdated,
// metricDetail) deliberately mirror frontends/omarchy/Model.js so the two native panels
// word the same data the same way.
//
// Two QML V4 engine rules apply to this file, asserted by the test:
//   - no ES2019 optional catch binding (`catch {`), which V4 rejects outright;
//   - no Unicode property escapes (\p{...}), which V4 evaluates to false
//     *silently* rather than throwing.

export const DEFAULT_BINARY = 'ai-monitor';
export const DEFAULT_TIMEOUT_SECS = 600;
export const MIN_TIMEOUT_SECS = 60;
export const MAX_TIMEOUT_SECS = 3600;

// The executable data engine hands QML no handle on the child process, so the
// applet cannot kill a hung binary: disconnectSource only stops us listening,
// and the process keeps running — one more of them every tick. Wrapping the
// spawn in timeout(1) is what actually bounds it, and is the nearest equivalent
// to the GNOME extension's proc.force_exit(). -k escalates to SIGKILL for a
// binary that ignores SIGTERM. coreutils provides timeout on any system that
// can run Plasma, and the QML watchdog stays on as a backstop for the case
// where the engine itself never reports back.
export const TIMEOUT_KILL_GRACE_SECS = 5;
export const EXIT_TIMED_OUT = 124;  // timeout(1)'s own code, after SIGTERM
export const EXIT_KILLED = 137;     // 128 + SIGKILL, after -k escalated

// KProcess::setShellCommand runs the source name through
// KShell::splitArgs(AbortOnMeta), which REFUSES the whole string when it meets
// an unquoted metacharacter. Single-quote everything; the closing/escaping
// dance is the only portable way to carry an embedded quote through.
export function shellQuote(value) {
    return `'${String(value ?? '').replace(/'/g, `'\\''`)}'`;
}

// One call returns every configured vendor, so the applet never passes
// --vendor and never reads ~/.cache/ai-monitor/active_vendor — that file
// belongs to the Waybar module's --cycle-next. Which vendor this instance
// shows is decided here, client side, from its own KConfigXT value.
export function timeoutSeconds(value) {
    if (value === null || value === undefined || String(value).trim() === '')
        return DEFAULT_TIMEOUT_SECS;
    const seconds = Number(value);
    if (!Number.isFinite(seconds))
        return DEFAULT_TIMEOUT_SECS;
    return Math.max(MIN_TIMEOUT_SECS, Math.min(MAX_TIMEOUT_SECS, Math.round(seconds)));
}

export function buildArgv(binary, timeoutSecs, options) {
    const bin = String(binary ?? '').trim() || DEFAULT_BINARY;
    const call = [bin, 'usage', '--json'];
    if (options && options.refresh)
        call.push('--refresh');
    if (options && options.account)
        call.push('--account', String(options.account));
    if (options && options.provider)
        call.push('--provider', String(options.provider));
    if (options && options.allProviders)
        call.push('--all-providers');
    return ['timeout', '-k', String(TIMEOUT_KILL_GRACE_SECS), String(timeoutSeconds(timeoutSecs))]
        .concat(call);
}

export function buildCommand(binary, timeoutSecs, options) {
    return buildArgv(binary, timeoutSecs, options).map(shellQuote).join(' ');
}

// Credential/session discovery is intentionally separate from `usage`: it
// performs local file/keyring reads only and never starts provider requests.
export function buildSessionCommand(binary, timeoutSecs) {
    const bin = String(binary ?? '').trim() || DEFAULT_BINARY;
    return ['timeout', '-k', String(TIMEOUT_KILL_GRACE_SECS), String(timeoutSeconds(timeoutSecs)),
        bin, 'sessions', '--json'].map(shellQuote).join(' ');
}

export function buildAccountSwitchCommand(binary, label) {
    const bin = String(binary ?? '').trim() || DEFAULT_BINARY;
    return [bin, 'account', 'switch', String(label ?? '')].map(shellQuote).join(' ');
}

export function buildAccountRemoveCommand(binary, label) {
    const bin = String(binary ?? '').trim() || DEFAULT_BINARY;
    return [bin, 'account', 'remove', String(label ?? '')].map(shellQuote).join(' ');
}

export function buildProviderToggleCommand(binary, slug, enable) {
    const bin = String(binary ?? '').trim() || DEFAULT_BINARY;
    const action = enable ? 'enable' : 'disable';
    return [bin, 'provider', action, String(slug ?? '')].map(shellQuote).join(' ');
}

export function buildProvidersListCommand(binary) {
    const bin = String(binary ?? '').trim() || DEFAULT_BINARY;
    return [bin, 'provider', 'list', '--json'].map(shellQuote).join(' ');
}

export function buildMonitorTestCommand(binary) {
    const bin = String(binary ?? '').trim() || DEFAULT_BINARY;
    return [bin, 'monitor', '--test'].map(shellQuote).join(' ');
}

export function buildSimulateRenewalCommand(binary) {
    const bin = String(binary ?? '').trim() || DEFAULT_BINARY;
    return [bin, 'monitor', '--simulate-renewal'].map(shellQuote).join(' ');
}

export function buildClearRenewalsCommand(binary) {
    const bin = String(binary ?? '').trim() || DEFAULT_BINARY;
    return [bin, 'monitor', '--clear-renewals'].map(shellQuote).join(' ');
}

export function formatAccount(labelOrUser, showFullEmail = true) {
    const s = String(labelOrUser ?? '').trim();
    if (!showFullEmail && s.indexOf('@') !== -1)
        return s.split('@')[0];
    return s;
}

// Launching the TUI needs a terminal, and there is no portable way to probe
// PATH from QML. Emit a shell fallback chain instead and let KProcess take its
// /bin/sh -c path — here the metacharacters are the point, unlike buildCommand
// where they had to be quoted away. Mirrors the candidate list the GNOME
// extension already walks.
export function buildTuiCommand(terminalCommand, tui = 'ai-monitor-tui') {
    const custom = String(terminalCommand ?? '').trim();
    if (custom)
        return `${custom} ${shellQuote(tui)}`;
    const q = shellQuote(tui);
    return [
        `konsole -e ${q}`,
        `gnome-terminal -- ${q}`,
        `xterm -e ${q}`,
    ].join(' || ');
}

// ---------------------------------------------------------------------------
// The report
// ---------------------------------------------------------------------------

export function safeText(value, maxLength = 400) {
    // Report fields ultimately come from remote provider responses. QML Labels
    // default to AutoText, where an HTML-looking value can become rich text and
    // load an inline image. The views also opt into Text.PlainText, but remove
    // markup delimiters and display-control characters here as defence in depth
    // for controls (buttons and check boxes) that expose no textFormat property.
    const s = String(value ?? '')
        .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f\u202a-\u202e\u2066-\u2069]/g, '')
        .replace(/</g, '‹')
        .replace(/>/g, '›');
    return s.length > maxLength ? s.slice(0, maxLength) : s;
}

function finitePercent(value) {
    // Number(null) === 0 in JS: without this guard a metric the Rust core
    // reports with percent null (a balance-style row, or a window the vendor
    // does not report) would normalize into a fabricated 0% bar instead of
    // keeping the null that means "not reported".
    if (value === null || value === undefined)
        return null;
    const n = Number(value);
    return Number.isFinite(n) ? Math.max(0, Math.min(100, n)) : null;
}

// Severity comes from the Rust core, which computes it once for every frontend.
// Fall back to the documented 50/75/90 bands only when a field is missing or
// unrecognised, so an older binary still renders sensible colours instead of
// everything reading as "low".
export const SEVERITIES = ['low', 'mid', 'high', 'critical'];

export function severityOf(percent, declared) {
    const s = String(declared ?? '');
    if (SEVERITIES.indexOf(s) >= 0)
        return s;
    const p = finitePercent(percent);
    if (p === null)
        return 'low';
    return p >= 90 ? 'critical' : p >= 75 ? 'high' : p >= 50 ? 'mid' : 'low';
}

export function severityColor(severity, colors) {
    const c = colors || {};
    switch (severity) {
    case 'critical': return c.critical;
    case 'high':     return c.high;
    case 'mid':      return c.mid;
    default:         return c.low;
    }
}

function normalizeSection(raw) {
    const type = String(raw && raw.type || '');
    if (type === 'spacer')
        return {type: 'spacer'};
    if (type === 'metric') {
        const percent = finitePercent(raw.percent);
        return {
            type: 'metric',
            label: safeText(raw.label, 120),
            value: safeText(raw.value, 40),
            percent: percent,
            detail: safeText(raw.detail, 400),
            resetAt: safeText(raw.reset_at, 64),
            windowSecs: Number.isFinite(raw.window_secs) ? Number(raw.window_secs) : null,
            severity: severityOf(percent, raw.severity),
        };
    }
    if (type === 'block') {
        const body = Array.isArray(raw.body)
            ? raw.body.slice(0, 128).map(line => safeText(line, 200)) : [];
        return {type: 'block', label: safeText(raw.label, 120), body: body};
    }
    // Unknown types are carried as plain text rather than dropped: a future
    // section kind should degrade to something readable, not vanish.
    return {
        type: 'text',
        label: safeText(raw && raw.label, 120),
        value: safeText(raw && raw.value, 200),
    };
}

// One provider in the `accounts[].providers` / `panel.items[].providers` shape.
function normalizeProvider(p) {
    if (!p || typeof p !== 'object') return null;
    return {
        id: safeText(p.id, 60),
        name: safeText(p.name, 60),
        icon: safeText(p.icon, 30),
        metrics: Array.isArray(p.metrics)
            ? p.metrics.map(m => ({
                label: safeText(m.label, 60),
                percent: typeof m.percent === 'number' ? m.percent : null,
                value: safeText(m.value, 30),
                severity: safeText(m.severity, 20),
                detail: safeText(m.detail, 60),
                windowSecs: Number.isFinite(m.window_secs) ? Number(m.window_secs) : null,
                resetAt: safeText(m.reset_at, 40) || null,
            }))
            : [],
        error: p.error ? safeText(p.error, 200) : null,
        recent: p.recent === true
    };
}

const PANEL_DEFAULTS = Object.freeze({
    mode: 'expanded', unit: 'provider', count: 3, interval: 5, hover: 'blocks',
    showFullEmail: true, showAccountName: true, showExtraModels: false, refreshInterval: 300, items: [],
});

// The `panel` object the binary builds from [display]. Every field is checked
// here so the QML never branches on a malformed value; an absent panel (an
// older binary) yields the defaults and no items, and the caller falls back.
export function normalizePanel(raw) {
    if (!raw || typeof raw !== 'object')
        return Object.assign({}, PANEL_DEFAULTS);
    const pick = (value, allowed, fallback) => allowed.indexOf(value) !== -1 ? value : fallback;
    const int = (value, lo, hi, fallback) => Number.isFinite(value)
        ? Math.min(hi, Math.max(lo, Math.trunc(value))) : fallback;
    const items = Array.isArray(raw.items) ? raw.items.slice(0, 64).map(function(item) {
        if (!item || typeof item !== 'object') return null;
        const key = safeText(item.key, 120).trim();
        if (!key) return null;
        return {
            key: key,
            title: safeText(item.title, 120) || key,
            account: item.account ? safeText(item.account, 120) : '',
            active: item.active === true,
            providers: Array.isArray(item.providers)
                ? item.providers.slice(0, 32).map(normalizeProvider).filter(Boolean) : [],
        };
    }).filter(Boolean) : [];
    return {
        mode: pick(raw.mode, ['expanded', 'carousel'], PANEL_DEFAULTS.mode),
        unit: pick(raw.unit, ['provider', 'account'], PANEL_DEFAULTS.unit),
        count: int(raw.count, 1, 6, PANEL_DEFAULTS.count),
        interval: int(raw.interval, 2, 300, PANEL_DEFAULTS.interval),
        hover: pick(raw.hover, ['blocks', 'pager'], PANEL_DEFAULTS.hover),
        showFullEmail: raw.show_full_email !== false,
        showAccountName: raw.show_account_name !== false,
        showExtraModels: raw.show_extra_models === true,
        refreshInterval: int(raw.refresh_interval, 5, 3600, PANEL_DEFAULTS.refreshInterval),
        items: items,
    };
}

function normalizeAccount(raw) {
    if (!raw || typeof raw !== 'object')
        return null;
    const label = safeText(raw.label, 120).trim();
    const user = safeText(raw.user, 120).trim();
    if (!label && !user)
        return null;
    const providers = Array.isArray(raw.providers)
        ? raw.providers.map(normalizeProvider).filter(Boolean)
        : [];
    return {
        label: label || user,
        user: user || label,
        active: raw.active === true,
        providers: providers,
    };
}

function normalizeEntry(raw) {
    if (!raw || typeof raw !== 'object')
        return null;
    const id = safeText(raw.id, 60).trim();
    if (!id)
        return null;
    const sections = Array.isArray(raw.sections)
        ? raw.sections.slice(0, 128).map(normalizeSection) : [];
    const extraModels = Array.isArray(raw.extra_models)
        ? raw.extra_models.slice(0, 32).map(m => safeText(m, 100)) : [];
    return {
        id: id,
        // display_name is the canonical label the Rust core owns. Falling back
        // to the raw id keeps a slug visible rather than an empty tab.
        label: safeText(raw.display_name, 60).trim() || safeText(raw.name, 60).trim() || id,
        plan: safeText(raw.plan, 80),
        status: safeText(raw.status, 24) || 'ready',
        stale: raw.stale === true,
        error: safeText(raw.error, 500),
        fetchedAt: safeText(raw.fetched_at, 64),
        sections: sections,
        extraModels: extraModels,
    };
}

// The binary always exits 0 and prints a report, so anything unparseable here
// is a missing binary or a crash — never a vendor-side failure, which arrives
// as an entry with status "error".
export function parseReport(stdout) {
    const raw = String(stdout ?? '').trim();
    if (!raw)
        return {ok: false, raw: '', entries: [], primary: '', accounts: [], panel: normalizePanel(null)};
    let parsed;
    try {
        parsed = JSON.parse(raw);
    } catch (e) {
        return {ok: false, raw: raw, entries: [], primary: '', accounts: [], panel: normalizePanel(null)};
    }
    if (!parsed || typeof parsed !== 'object' || !Array.isArray(parsed.entries))
        return {ok: false, raw: raw, entries: [], primary: '', accounts: [], panel: normalizePanel(null)};
    const accounts = Array.isArray(parsed.accounts)
        ? parsed.accounts.slice(0, 32).map(normalizeAccount).filter(Boolean) : [];
    const renewals = Array.isArray(parsed.renewals) ? parsed.renewals : [];
    const account = parsed.account && typeof parsed.account === 'object'
        ? {
            label: safeText(parsed.account.label, 120).trim(),
            user: safeText(parsed.account.user, 120).trim(),
            providers: Array.isArray(parsed.account.providers)
                ? parsed.account.providers.map(p => safeText(p, 60))
                : [],
        }
        : null;
    return {
        ok: true,
        raw: raw,
        entries: parsed.entries.slice(0, 64).map(normalizeEntry).filter(Boolean),
        primary: safeText(parsed.primary, 60).trim(),
        account: account,
        accounts: accounts,
        renewals: renewals,
        panel: normalizePanel(parsed.panel),
    };
}

export function parseProviders(stdout) {
    const raw = String(stdout ?? '').trim();
    if (!raw)
        return [];
    let parsed;
    try {
        parsed = JSON.parse(raw);
    } catch (e) {
        return [];
    }
    if (!Array.isArray(parsed))
        return [];
    return parsed.map(p => ({
        id: safeText(p.id || p.slug, 40).trim(),
        name: safeText(p.name || p.display_name, 60).trim(),
        enabled: p.enabled === true,
        configured: p.configured === true,
    })).filter(p => !!p.id);
}

// Which entry this applet instance shows. The configured vendor wins; the
// report's own primary is the fallback, then the first entry. Never throws on
// an id the report no longer carries — a vendor removed from config.toml must
// degrade to something visible, not to a blank panel.
export function entryFor(report, vendorId) {
    const entries = (report && report.entries) || [];
    if (!entries.length)
        return null;
    const wanted = String(vendorId ?? '').trim();
    for (const e of entries)
        if (e.id === wanted)
            return e;
    const primary = String((report && report.primary) ?? '').trim();
    for (const e of entries)
        if (e.id === primary)
            return e;
    return entries[0];
}

// The popup's tab strip. Every configured vendor is offered, including ones
// currently failing — hiding an errored vendor is what made it impossible to
// tell "not configured" from "configured and broken".
export function vendorTabs(report, activeId) {
    const entries = (report && report.entries) || [];
    const active = entryFor(report, activeId);
    return entries.map(e => ({
        id: e.id,
        label: e.label,
        active: !!active && e.id === active.id,
        failing: e.status === 'error' || !!e.error,
    }));
}

export function nextVendor(ring, current, delta) {
    const list = toRing(ring);
    if (!list.length)
        return current;
    const step = Number(delta) || 0;
    const at = list.indexOf(current);
    if (at === -1)
        return list[0];
    const n = list.length;
    return list[(((at + step) % n) + n) % n];
}

// A KConfigXT StringList reaches QML as an array-LIKE object: right length,
// right contents, but Array.isArray() === false. Guarding on isArray made
// every scroll a silent no-op in the real panel.
function toRing(ring) {
    if (ring === null || ring === undefined || typeof ring === 'string')
        return [];
    const len = Number(ring.length);
    if (!Number.isFinite(len) || len <= 0)
        return [];
    return Array.from(ring);
}

// ---------------------------------------------------------------------------
// Projection
// ---------------------------------------------------------------------------

// The worst metric, which is what the panel shows when it has room for one
// number. A balance-style row reports money rather than a percentage, so it
// contributes its own value text instead.
export function headline(entry) {
    if (!entry)
        return {text: '', percent: null, severity: 'low', label: ''};
    let best = null;
    for (const s of entry.sections)
        if (s.type === 'metric' && s.percent !== null && (!best || s.percent > best.percent))
            best = s;
    if (best) {
        const isBalance = /balance/i.test(best.label) && best.value !== '';
        return {
            text: isBalance ? best.value : `${best.percent}%`,
            percent: best.percent,
            severity: best.severity,
            label: best.label,
        };
    }
    for (const s of entry.sections)
        if (s.type === 'text' && /(balance|available|spend|prepaid)/i.test(s.label) && s.value !== '')
            return {text: s.value, percent: null, severity: 'low', label: s.label};
    return {
        text: entry.status === 'error' ? 'Error' : 'Ready',
        percent: null,
        severity: 'low',
        label: '',
    };
}

export function isAlarming(entry) {
    if (!entry)
        return false;
    return entry.status === 'error' || entry.stale === true || headline(entry).severity === 'critical';
}

// The metric rows the popup renders, spacers dropped: Column spacing handles
// the rhythm, so a spacer would double it.
export function detailRows(entry, showExtraModels = true) {
    if (!entry)
        return [];
    const isAgy = entry.id === 'antigravity';
    return entry.sections.filter(s => {
        if (s.type === 'spacer') return false;
        if (!showExtraModels && isAgy && /claude|gpt/i.test(s.label)) return false;
        if (s.type === 'text') {
            const lbl = String(s.label || '').trim();
            const val = String(s.value || '').trim();
            if (lbl.startsWith('HTTP ') || lbl === 'Error' || lbl === 'Warning') {
                if (val.startsWith('{') || val.includes('not logged into') || val.includes('error getting token')) {
                    return false;
                }
            }
            if (val.startsWith('{"code":') || val.includes('failed to get load code assist response')) {
                return false;
            }
        }
        return true;
    });
}

// The panel cells. Each carries its own severity so the compact representation
// colours per cell rather than by the entry's worst value.
export function panelCells(entry, options) {
    if (!entry)
        return [];
    if (entry.status === 'error')
        return [{providerId: entry.id, label: '', text: '⚠', severity: 'critical', percent: null, isExtra: false}];

    const opts = options || {};
    const isAgy = entry.id === 'antigravity';
    const showWeekly = (opts.showWeekly !== undefined)
        ? Boolean(opts.showWeekly)
        : (isAgy && opts.max !== undefined ? false : true);
    const showExtra = opts.showExtraModels !== false;
    const defaultMax = (showWeekly && showExtra && isAgy) ? 3 : (showWeekly ? 2 : (isAgy && showExtra ? 2 : 1));
    const max = Number.isFinite(Number(opts.max)) ? Math.max(1, Number(opts.max)) : defaultMax;
    let sections = entry.sections;
    if (!sections && Array.isArray(entry.metrics)) {
        sections = entry.metrics.map(function(m) {
            return {
                type: 'metric',
                label: m.label,
                percent: m.percent,
                value: m.value,
                severity: m.severity,
                detail: m.detail,
                resetAt: m.resetAt || m.reset_at,
                windowSecs: m.windowSecs || m.window_secs
            };
        });
    }
    if (!sections) sections = [];

    if (isAgy) {
        let currentWindow = 'session';
        const agyMetrics = [];
        for (const s of sections) {
            if (s.type === 'text') {
                const lbl = String(s.label || '').toLowerCase();
                if (lbl.includes('weekly') || lbl.includes('semanal') || lbl.includes('7d')) {
                    currentWindow = 'weekly';
                } else if (lbl.includes('session') || lbl.includes('5h')) {
                    currentWindow = 'session';
                }
            }
            if (s.type !== 'metric')
                continue;

            let win = currentWindow;
            if (s.windowSecs) {
                win = (s.windowSecs > 86400) ? 'weekly' : 'session';
            } else if (/weekly|7d|semanal/i.test(s.label)) {
                win = 'weekly';
            } else if (/session|5h/i.test(s.label)) {
                win = 'session';
            }

            const isExtra = /claude|gpt/i.test(s.label);
            agyMetrics.push({
                s: s,
                window: win,
                isExtra: isExtra,
            });
        }

        // If no explicit window tags found and 4 metrics exist (e.g. test fixtures), indices 2+ are weekly
        if (agyMetrics.length >= 4 && agyMetrics.every(m => m.window === 'session')) {
            for (let i = 2; i < agyMetrics.length; i++) {
                agyMetrics[i].window = 'weekly';
            }
        }

        const primarySession = agyMetrics.find(m => m.window === 'session' && !m.isExtra);
        const primaryWeekly = agyMetrics.find(m => m.window === 'weekly' && !m.isExtra);
        const extraSession = agyMetrics.find(m => m.window === 'session' && m.isExtra);
        const extraWeekly = agyMetrics.find(m => m.window === 'weekly' && m.isExtra);

        const cells = [];

        if (showWeekly) {
            if (primarySession) {
                const pLabel = shortLabel(primarySession.s.label) || 'Gemini';
                cells.push({
                    providerId: entry.id,
                    label: pLabel,
                    text: primarySession.s.percent === null ? primarySession.s.value : `${primarySession.s.percent}%`,
                    severity: primarySession.s.severity,
                    percent: primarySession.s.percent,
                    isExtra: false,
                });
            }
            if (primaryWeekly) {
                cells.push({
                    providerId: entry.id,
                    label: '7d',
                    text: primaryWeekly.s.percent === null ? primaryWeekly.s.value : `${primaryWeekly.s.percent}%`,
                    severity: primaryWeekly.s.severity,
                    percent: primaryWeekly.s.percent,
                    isExtra: false,
                });
            }
            if (showExtra && (extraSession || extraWeekly)) {
                const ex = extraSession || extraWeekly;
                cells.push({
                    providerId: entry.id,
                    label: shortLabel(ex.s.label),
                    text: ex.s.percent === null ? ex.s.value : `${ex.s.percent}%`,
                    severity: ex.s.severity,
                    percent: ex.s.percent,
                    isExtra: true,
                });
            }
        } else {
            // Only 5h
            if (primarySession) {
                cells.push({
                    providerId: entry.id,
                    label: shortLabel(primarySession.s.label) || 'Gemini',
                    text: primarySession.s.percent === null ? primarySession.s.value : `${primarySession.s.percent}%`,
                    severity: primarySession.s.severity,
                    percent: primarySession.s.percent,
                    isExtra: false,
                });
            }
            if (showExtra && extraSession) {
                cells.push({
                    providerId: entry.id,
                    label: shortLabel(extraSession.s.label),
                    text: extraSession.s.percent === null ? extraSession.s.value : `${extraSession.s.percent}%`,
                    severity: extraSession.s.severity,
                    percent: extraSession.s.percent,
                    isExtra: true,
                });
            }
        }

        return cells.slice(0, max);
    }

    // Generic / non-antigravity providers. Keep the 7-day window visible
    // whenever the user enabled it, even when a provider inserts another
    // metric (for example a monthly pool) before its weekly quota.
    const candidates = [];
    let currentWindow = 'session';
    for (const s of sections) {
        if (s.type === 'text') {
            const lbl = String(s.label || '').toLowerCase();
            if (lbl.includes('weekly') || lbl.includes('semanal') || lbl.includes('7d')) {
                currentWindow = 'weekly';
            } else if (lbl.includes('session') || lbl.includes('5h')) {
                currentWindow = 'session';
            }
        }
        if (s.type !== 'metric')
            continue;

        let win = currentWindow;
        if (s.windowSecs) {
            // Only a real seven-day duration is weekly. A 30-day/monthly
            // pool must not consume the compact 7d slot.
            if (Math.abs(Number(s.windowSecs) - 604800) < 60) {
                win = 'weekly';
            } else if (Number(s.windowSecs) > 86400) {
                win = 'other';
            } else {
                win = 'session';
            }
        } else if (/weekly|7d|semanal/i.test(s.label)) {
            win = 'weekly';
        } else if (/monthly|month|30d/i.test(s.label)) {
            win = 'other';
        } else if (/session|5h/i.test(s.label)) {
            win = 'session';
        }

        if (!showWeekly && win === 'weekly')
            continue;

        candidates.push({
            providerId: entry.id,
            // All compact weekly metrics use one stable label, independent
            // of the provider's original wording (Weekly, Codex, etc.).
            label: win === 'weekly' ? '7d' : sessionLabel(entry, s.label),
            text: s.percent === null ? s.value : `${s.percent}%`,
            severity: s.severity,
            percent: s.percent,
            isExtra: false,
            window: win,
        });
    }

    if (!showWeekly)
        return candidates.slice(0, max);

    // The primary session and first 7d window are the compact contract.
    // Remaining metrics fill only spare slots, retaining report order.
    const selected = [];
    const session = candidates.find(c => c.window === 'session');
    const weekly = candidates.find(c => c.window === 'weekly');
    if (session) selected.push(session);
    if (weekly) selected.push(weekly);
    for (const candidate of candidates) {
        if (selected.indexOf(candidate) === -1)
            selected.push(candidate);
    }
    return selected.slice(0, max);
}

// The card view (viewMode "VendorCards") projects one card per entry the
// aggregate report already returned — never a hardcoded vendor table. The
// Rust core owns names, metric projection and severity bands; this only
// restates what `usage --json` handed over in a shape the QML can lay out.
function severityRank(severity) {
    return SEVERITIES.indexOf(severityOf(null, severity));
}

export function cardState(entry) {
    if (!entry)
        return 'ok';
    if (entry.status === 'error' || entry.error)
        return 'error';
    if (entry.stale === true)
        return 'stale';
    return 'ok';
}

// One card's view model. Windows are the entry's metric sections; a window
// with percent === null renders as "not reported" rather than as a fabricated
// 0% bar. Block sections (balances, free-form notes) ride along untouched so
// the card shows everything the tab view would.
export function cardFor(entry) {
    if (!entry)
        return null;
    let worst = -1;
    let accent = 'low';
    for (const s of entry.sections) {
        if (s.type !== 'metric')
            continue;
        const rank = severityRank(s.severity);
        if (rank > worst) {
            worst = rank;
            accent = SEVERITIES[worst];
        }
    }
    const state = cardState(entry);
    return {
        id: entry.id,
        label: entry.label,
        plan: entry.plan,
        state: state,
        // An errored vendor outranks whatever its last good numbers said.
        accent: state === 'error' ? 'critical' : accent,
        error: state === 'error' ? errorMessage(entry.error) : '',
        extraModels: entry.extraModels || [],
        windows: entry.sections.filter(s => s.type === 'metric').map(s => ({
            label: s.label,
            window: shortLabel(s.label),
            percent: s.percent,
            value: s.percent === null ? s.value : `${s.percent}%`,
            severity: s.severity,
            resetAt: s.resetAt,
            detail: metricDetail(s),
        })),
        blocks: entry.sections.filter(s => s.type === 'block'),
    };
}

export function cardModel(report) {
    const entries = (report && report.entries) || [];
    return entries.map(cardFor);
}

// "Session (5h)" -> "5h", "Weekly (7d)" -> "7d". The panel is width
// constrained; the full label lives in the popup. Falls back to the first word
// so an unrecognised label still shortens to something rather than to nothing.
// "Session (5h)" shortens to a bare window, which on the bar reads as a
// number with no owner next to "Codex 18%" and "Gemini 4%". A label that is
// only a window names the provider instead, from the Rust display name.
function sessionLabel(entry, label) {
    const short = shortLabel(label);
    if (/^(\d+\s*[hmd]|session|sessão)$/i.test(short)) {
        const name = safeText(entry && (entry.label || entry.name), 60).trim();
        if (name)
            return name;
    }
    return short;
}

export function shortLabel(label) {
    const s = String(label ?? '').trim();
    const paren = s.match(/\(([^)]{1,8})\)\s*$/);
    if (paren)
        return paren[1];
    return s.split(/\s+/)[0] || '';
}

// detail still carries a human-readable "Resets in …" written for CLI readers.
// The panel renders a live countdown from reset_at instead, so that fragment
// would be both stale and duplicated. Mirrors frontends/omarchy/Model.js metricDetail.
export function metricDetail(row) {
    let detail = safeText(row && row.detail, 1000);
    if (!row || !row.resetAt)
        return detail.trim();
    detail = detail.replace(/^Resets in [^·]+\s*(?:·\s*)?/i, '');
    detail = detail.replace(/\s*·\s*reset\s+[^·]+$/i, '');
    return detail.trim();
}

export function formatDuration(milliseconds) {
    const ms = Number(milliseconds);
    if (!(ms > 0))
        return 'now';
    const minutes = Math.floor(ms / 60000);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);
    if (days > 0)
        return `${days}d ${hours % 24}h`;
    if (hours > 0)
        return `${hours}h ${minutes % 60}m`;
    return `${Math.max(1, minutes)}m`;
}

// Measured against a locally ticking clock rather than baked into the report,
// so the countdown stays live between fetches. That is what lets the refresh
// interval be minutes rather than seconds without the popup looking frozen.
//
// These return milliseconds, not sentences: the words are i18n()'d in QML, and
// keeping the arithmetic here is what makes it testable under Node. Returns
// null when the timestamp is absent or unparseable, which the caller renders as
// "unknown" rather than as an accidental "0m".
export function resetRemainingMs(resetAt, nowMs) {
    if (!resetAt)
        return null;
    const resetMs = new Date(String(resetAt)).getTime();
    if (!Number.isFinite(resetMs))
        return null;
    return resetMs - Number(nowMs);
}

export function updatedAgeMs(fetchedAt, nowMs) {
    if (!fetchedAt)
        return null;
    const fetchedMs = new Date(String(fetchedAt)).getTime();
    if (!Number.isFinite(fetchedMs))
        return null;
    return Math.max(0, Number(nowMs) - fetchedMs);
}

export function errorMessage(value) {
    const message = safeText(value, 500).trim();
    return message === '' ? 'The usage command failed without an error message.' : message;
}

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

// Referencing the Kirigami roles is what makes the applet follow the Plasma
// colour scheme. Mapping severity onto the semantic roles rather than onto
// fixed hexes is what makes a Breeze Light user see Breeze Light.
export function paletteFromTheme(theme) {
    const t = theme || {};
    return {
        low: t.positiveTextColor || t.textColor,
        mid: t.neutralTextColor || t.textColor,
        high: t.neutralTextColor || t.textColor,
        critical: t.negativeTextColor || t.textColor,
        empty: t.disabledTextColor || t.textColor,
    };
}

// Only start a fetch when the previous one is done, and never let a config
// change queue a second in-flight command: the source name IS the command, so
// two of them would race to paint the same panel.
export function shouldStartFetch(pendingCommand, nextCommand) {
    return String(pendingCommand ?? '') === '' && String(nextCommand ?? '') !== '';
}

export function filterActiveRenewals(renewals, dismissedIds) {
    if (!Array.isArray(renewals)) return [];
    var dismissed = Array.isArray(dismissedIds) ? dismissedIds : [];
    return renewals.filter(function(r) {
        return r && r.id && dismissed.indexOf(r.id) === -1;
    });
}

// Several quota windows can renew in the same account/provider cycle.  Keep
// them together in the UI instead of presenting a stack of nearly identical
// cards (Gemini 5h, Gemini weekly, and the Claude/GPT extra pool).
export function groupActiveRenewals(renewals) {
    if (!Array.isArray(renewals)) return [];
    // Deliberately one display item per event: account segregation applies to
    // usage cards only, never to renewal notifications.
    return renewals.map(function(renewal) {
        return {
            id: renewal.id,
            account_label: renewal.account_label,
            provider_id: renewal.provider_id,
            provider_name: renewal.provider_name,
            renewals: [renewal]
        };
    });
}

export function renewalWindowDescription(renewal) {
    if (!renewal) return "";
    var provider = String(renewal.provider_id || renewal.provider_name || "").toLowerCase();
    var isExtra = (provider === "antigravity" || provider.indexOf("antigravity") !== -1) &&
        /(claude|gpt)/i.test(String(renewal.metric_label || ""));
    var isWeekly = /(semana|weekly)/i.test(String(renewal.window_type || ""));
    var pool = isExtra ? "modelo extra" : "modelo normal";
    var window = isWeekly ? "janela semanal" : "janela de 5h";
    return pool + " — " + window;
}

export function formatRenewalSummary(renewal, showFullEmail) {
    if (!renewal) return "";
    var acct = formatAccount(renewal.account_label, showFullEmail);
    var prov = renewal.provider_name || renewal.provider_id || "AI";
    var metric = renewal.metric_label || "Cota";
    return acct + " [" + prov + "]: " + metric + " (" + renewalWindowDescription(renewal) + ")";
}

// Wrap a carousel/pager position into [0, n).
export function wrapIndex(position, n) {
    if (!(n > 0)) return 0;
    const p = Math.trunc(Number(position) || 0);
    return ((p % n) + n) % n;
}

// The panel items on screen now: the first `count` side by side, or the one
// at `position` in the carousel. Order and eligibility come from the binary.
export function visiblePanelItems(panel, position) {
    const items = (panel && panel.items) || [];
    if (items.length === 0) return [];
    if (panel.mode === 'carousel')
        return [items[wrapIndex(position, items.length)]];
    return items.slice(0, Math.max(1, panel.count || 1));
}

// The accounts in use right now, for the click popup: those with a recently
// used provider, narrowed to those providers. With no recency information
// (one session per provider), the active account, else every account.
export function inUseAccounts(accounts) {
    const list = accounts || [];
    const inUse = list.map(function(a) {
        const providers = (a.providers || []).filter(function(p) { return p.recent === true; });
        return Object.assign({}, a, {providers: providers});
    }).filter(function(a) { return a.providers.length > 0; });
    if (inUse.length > 0) return inUse;
    const active = list.filter(function(a) { return a.active; });
    return active.length > 0 ? active : list;
}

// `settings show`: the non-secret settings snapshot, including [display].
export function buildSettingsShowCommand(binary) {
    const bin = String(binary ?? '').trim() || DEFAULT_BINARY;
    return [bin, 'settings', 'show'].map(shellQuote).join(' ');
}

// `settings apply` reads its patch from stdin. The executable engine refuses
// an unquoted `|`, so the pipe lives inside a quoted `sh -c` script and the
// binary and JSON travel as its positional arguments, never spliced into it.
export function buildSettingsApplyCommand(binary, patch) {
    const bin = String(binary ?? '').trim() || DEFAULT_BINARY;
    const json = JSON.stringify(Object.assign({schema_version: 1}, patch || {}));
    return ['sh', '-c', 'printf "%s\\n" "$1" | "$0" settings apply', bin, json]
        .map(shellQuote).join(' ');
}

// The dismissed ids still worth remembering: those the report still carries.
// The binary owns dismissal (`monitor --clear-renewals` records it), so this
// list only bridges the gap until the next report and must not grow forever.
export function pruneDismissed(dismissedIds, renewals) {
    const live = (renewals || []).map(function(r) { return r && r.id; });
    return (dismissedIds || []).filter(function(id) { return live.indexOf(id) !== -1; });
}
