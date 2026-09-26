pragma ComponentBehavior: Bound

import QtQuick
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.plasma5support as Plasma5Support
import org.kde.kirigami as Kirigami
import "../code/plasmoid-logic.mjs" as Logic

PlasmoidItem {
    id: root

    // --- state -------------------------------------------------------------
    property var report: null       // parsed `usage --json`, or null
    property string failure: ""     // our failure: spawn, timeout, unparseable

    // The command currently in flight, "" when idle. Also the watchdog's handle
    // on which source to drop.
    property string pendingCommand: ""
    // A refresh requested during an in-flight fetch is coalesced into one
    // follow-up. This protects the panel from slow providers and prevents a
    // 10-second session tick from discarding a healthy response.
    property bool refreshQueued: false
    property bool queuedRefreshForced: false
    property string queuedRefreshProvider: ""
    property bool queuedRefreshAllProviders: false

    // Ticked locally so every "resets in" and "updated N ago" counts down
    // between fetches. Without it the popup would look frozen for minutes at a
    // time, which is what makes a slow refresh interval acceptable at all.
    property double nowMs: 0

    // A report fetch can visit several providers sequentially, each with its
    // own network timeout. Keep this configurable and bounded rather than
    // killing a healthy multi-provider report after one minute.
    readonly property int fetchTimeoutSecs: Logic.timeoutSeconds(
        Plasmoid.configuration.commandTimeout)

    // The floor the settings actually offer: config/main.xml declares <min>5</min>
    // and the spinbox steps from 5 or 10. One report covers every configured vendor.
    readonly property int minIntervalSecs: 5

    readonly property string vendor: Plasmoid.configuration.vendor
    readonly property var entry: Logic.entryFor(root.report, root.vendor)
    readonly property var tabs: Logic.vendorTabs(root.report, root.vendor)
    readonly property int metricWindowMode: Plasmoid.configuration.metricWindowMode
    readonly property bool showWeekly: root.metricWindowMode === 0
    readonly property var compactCells: Logic.panelCells(root.entry, {
        showWeekly: root.showWeekly,
        showExtraModels: root.showExtraModels
    })
    readonly property var accounts: (root.report && root.report.accounts) || []
    property string activeAccountOverride: ""
    readonly property string currentAccountLabel: {
        function accountHasVendor(accLabel, vId) {
            if (!accLabel || !vId) return false;
            for (let i = 0; i < root.accounts.length; i++) {
                if (root.accounts[i].label === accLabel || root.accounts[i].user === accLabel) {
                    const provs = root.accounts[i].providers || [];
                    for (let j = 0; j < provs.length; j++) {
                        if (provs[j].id === vId) return true;
                    }
                }
            }
            return false;
        }

        const effectiveVendor = root.entry ? root.entry.id : root.vendor;

        // 1. If activeAccountOverride is set and valid for the current vendor, use it
        if (root.activeAccountOverride && (!effectiveVendor || accountHasVendor(root.activeAccountOverride, effectiveVendor))) {
            return root.activeAccountOverride;
        }

        // 2. If the report specified an active account and it supports the current vendor
        if (root.report && root.report.account && root.report.account.label) {
            if (!effectiveVendor || accountHasVendor(root.report.account.label, effectiveVendor)) {
                return root.report.account.label;
            }
        }

        // 3. Find an active account that supports the current vendor
        for (let i = 0; i < root.accounts.length; i++) {
            if (root.accounts[i].active && (!effectiveVendor || accountHasVendor(root.accounts[i].label, effectiveVendor))) {
                return root.accounts[i].label;
            }
        }

        // 4. Fallback to any account supporting the current vendor
        if (effectiveVendor) {
            for (let i = 0; i < root.accounts.length; i++) {
                if (accountHasVendor(root.accounts[i].label, effectiveVendor)) {
                    return root.accounts[i].label;
                }
            }
        }

        // 5. Ultimate fallback
        if (root.report && root.report.account && root.report.account.label)
            return root.report.account.label;
        for (let i = 0; i < root.accounts.length; i++) {
            if (root.accounts[i].active)
                return root.accounts[i].label;
        }
        return root.accounts.length > 0 ? root.accounts[0].label : "";
    }
    readonly property var rawRenewals: (root.report && root.report.renewals) || []
    property var dismissedRenewalIds: Plasmoid.configuration.dismissedRenewals || []
    property bool showRenewalNotice: false

    readonly property var activeRenewals: Logic.filterActiveRenewals(
        root.rawRenewals, root.dismissedRenewalIds)
    readonly property var activeRenewalGroups: Logic.groupActiveRenewals(root.activeRenewals)

    function dismissActiveRenewals() {
        // Only what is on screen now: anything older is gone from the report
        // and was already recorded as dismissed by the binary.
        const set = (root.rawRenewals || []).map(function(r) { return r && r.id; })
            .filter(function(id) { return !!id; });
        root.dismissedRenewalIds = set;
        Plasmoid.configuration.dismissedRenewals = set;
        root.showRenewalNotice = false;
        launcher.exec(Logic.buildClearRenewalsCommand(Plasmoid.configuration.binaryPath));
    }

    function triggerRenewalNoticeToggle() {
        root.showRenewalNotice = true;
        root.expanded = !root.expanded;
    }

    // Layout, item order and account settings come from config.toml
    // [display] via the report's `panel`, shared with every other frontend.
    readonly property var panel: (root.report && root.report.panel) || Logic.normalizePanel(null)
    readonly property bool showFullEmail: root.panel.showFullEmail
    // The report carries accounts only when multi-account is on.
    readonly property bool multiAccount: root.accounts.length > 0
    readonly property bool accountUnit: root.panel.unit === "account"
    readonly property bool showExtraModels: root.panel.showExtraModels
    readonly property var selectedExtraModels: Plasmoid.configuration.selectedExtraModels || []
    readonly property int compactDisplayMode: Plasmoid.configuration.compactDisplayMode

    readonly property bool carousel: root.panel.mode === "carousel"
    // One position for the carousel and the pager tooltip: the wheel steps it,
    // the carousel timer advances it, and visiblePanelItems wraps it.
    property int panelPosition: 0
    readonly property var visibleItems: Logic.visiblePanelItems(root.panel, root.panelPosition)
    // The item the pager tooltip shows.
    readonly property var pagerItem: root.panel.items.length > 0
        ? root.panel.items[Logic.wrapIndex(root.panelPosition, root.panel.items.length)] : null

    readonly property var compactEntries: {
        const cellOptions = {
            showWeekly: root.showWeekly,
            showExtraModels: root.showExtraModels
        };
        const list = [];
        root.visibleItems.forEach(function(item) {
            item.providers.forEach(function(p, pIdx) {
                const cells = Logic.panelCells({ id: p.id, name: p.name, metrics: p.metrics }, cellOptions);
                cells.forEach(function(c) { c.providerId = p.id; });
                list.push({
                    id: p.id,
                    label: root.accountUnit ? item.title : (p.name || p.id),
                    rawAccountLabel: item.account,
                    // An account item names itself once, ahead of its providers,
                    // unless [display] show_account_name is off.
                    accountTitle: root.accountUnit && root.panel.showAccountName && pIdx === 0
                        ? item.title : "",
                    cells: cells,
                    status: p.error ? "error" : "ok",
                    failure: !!p.error
                });
            });
        });
        if (list.length > 0 || root.panel.items.length > 0)
            return list;
        // An older binary without `panel`: the selected entry alone.
        const single = root.entry;
        return single ? [{
            id: single.id,
            label: single.label,
            accountTitle: "",
            cells: root.compactCells,
            status: single.status,
            failure: single.status === "error"
        }] : [];
    }

    // Read back as the integer index of the Enum choice (0 = tabs, 1 = cards).
    readonly property int viewMode: Plasmoid.configuration.viewMode
    // The cards view projects every entry the report returned; it needs no
    // per-vendor selection and never refetches when toggled.
    readonly property var cards: Logic.cardModel(root.report)

    // The user's own five colours, defaulting to One Dark exactly as in
    // src/core/theme.rs, the GNOME extension and the macOS app.
    readonly property var fixedColors: ({
        low: Plasmoid.configuration.colorLow,
        mid: Plasmoid.configuration.colorMid,
        high: Plasmoid.configuration.colorHigh,
        critical: Plasmoid.configuration.colorCritical,
        empty: Plasmoid.configuration.colorEmpty,
    })

    // Referencing the Kirigami roles inside the binding is what makes this
    // reactive: they are notifying properties, so a theme switch recolours with
    // no restart. Assigning imperatively anywhere would freeze the old colours.
    readonly property var colors: Plasmoid.configuration.useThemeColors
        ? Logic.paletteFromTheme({
            textColor: Kirigami.Theme.textColor,
            neutralTextColor: Kirigami.Theme.neutralTextColor,
            negativeTextColor: Kirigami.Theme.negativeTextColor,
            positiveTextColor: Kirigami.Theme.positiveTextColor,
            disabledTextColor: Kirigami.Theme.disabledTextColor,
        })
        : root.fixedColors

    // --- applet plumbing ---------------------------------------------------
    Plasmoid.icon: "speedometer"

    switchWidth: Kirigami.Units.gridUnit * 14
    switchHeight: Kirigami.Units.gridUnit * 10

    // When left click launches the TUI, the global shortcut and Enter/Space
    // must not still open a popup we have decided not to use.
    activationTogglesExpanded: Plasmoid.configuration.leftClickAction === 0
    Plasmoid.onActivated: {
        if (Plasmoid.configuration.leftClickAction === 1)
            root.launchTui();
    }

    compactRepresentation: CompactRepresentation { applet: root }
    fullRepresentation: FullRepresentation { applet: root }

    toolTipMainText: {
        const entries = (root.report && root.report.entries) || [];
        if (entries.length > 1) {
            return entries.map(function(e) { return e.label; }).join(" + ");
        }
        return root.entry ? root.entry.label : i18n("AI Monitor");
    }
    toolTipSubText: {
        if (root.failure) return root.failure;
        const entries = (root.report && root.report.entries) || [];
        if (entries.length > 1) {
            return entries.map(function(e) { return e.plan ? (e.label + ": " + e.plan) : e.label; }).join(" · ");
        }
        return root.entry ? (root.entry.plan || root.entry.status) : i18n("Loading…");
    }
    toolTipItem: UsageToolTip { applet: root }

    // Wording deliberately identical to the GNOME dropdown and the macOS menu.
    Plasmoid.contextualActions: [
        PlasmaCore.Action {
            text: i18n("Refresh now")
            icon.name: "view-refresh"
            onTriggered: root.refresh(true)
        },
        PlasmaCore.Action {
            text: i18n("Open TUI")
            icon.name: "utilities-terminal"
            onTriggered: root.launchTui()
        }
    ]

    // --- words -------------------------------------------------------------
    // The pure layer returns milliseconds; the sentences are built here so they
    // go through i18n() like the rest of the applet's chrome.
    function resetText(resetAt) {
        const ms = Logic.resetRemainingMs(resetAt, root.nowMs);
        if (ms === null)
            return "";
        return ms > 0 ? i18n("Resets in %1", Logic.formatDuration(ms))
                      : i18n("Reset due");
    }

    function updatedText() {
        const entries = (root.report && root.report.entries) || [];
        if (entries.length === 0 && !root.entry)
            return "";
        if (root.failure)
            return "";
        let bestMs = null;
        for (let i = 0; i < entries.length; i++) {
            const e = entries[i];
            if (e && e.fetchedAt && e.status !== "error" && !e.error) {
                const age = Logic.updatedAgeMs(e.fetchedAt, root.nowMs);
                if (age !== null && (bestMs === null || age < bestMs)) {
                    bestMs = age;
                }
            }
        }
        if (bestMs === null && root.entry && root.entry.fetchedAt) {
            bestMs = Logic.updatedAgeMs(root.entry.fetchedAt, root.nowMs);
        }
        if (bestMs === null)
            return i18n("Updated just now");
        const base = bestMs < 60000 ? i18n("Updated just now")
                                    : i18n("Updated %1 ago", Logic.formatDuration(bestMs));
        return root.pendingCommand !== "" ? base + i18n(" · refreshing…") : base;
    }

    // The one line that explains a non-normal state, "" when all is well.
    function statusMessage() {
        if (root.failure)
            return root.failure;
        if (!root.report)
            return "";
        const entries = (root.report && root.report.entries) || [];
        if (entries.length === 0 && !root.entry)
            return i18n("No configured provider reported usage.");
        if (entries.length > 0) {
            const errors = [];
            for (let i = 0; i < entries.length; i++) {
                if (entries[i].status === "error" || entries[i].error) {
                    errors.push(Logic.errorMessage(entries[i].error));
                }
            }
            if (errors.length === entries.length) {
                return errors[0] || i18n("No configured provider reported usage.");
            }
            return "";
        }
        if (root.entry && (root.entry.status === "error" || root.entry.error))
            return Logic.errorMessage(root.entry.error);
        return "";
    }

    function statusIsUrgent() {
        if (root.failure !== "")
            return true;
        const entries = (root.report && root.report.entries) || [];
        for (let i = 0; i < entries.length; i++) {
            if (entries[i] && (entries[i].status === "error" || !!entries[i].error))
                return true;
        }
        return !!root.entry && root.entry.status === "error";
    }

    // --- data --------------------------------------------------------------
    Plasma5Support.DataSource {
        id: reader
        engine: "executable"
        connectedSources: []

        onNewData: (sourceName, data) => {
            // MANDATORY: the source name IS the command string and stays
            // connected, re-running on the engine's own interval. Disconnecting
            // here turns every connectSource into a strict one-shot.
            disconnectSource(sourceName);
            if (sourceName !== root.pendingCommand)
                return;
            root.pendingCommand = "";
            watchdog.stop();
            root.consume(data);
            if (root.refreshQueued) {
                const forced = root.queuedRefreshForced;
                root.refreshQueued = false;
                root.queuedRefreshForced = false;
                const provider = root.queuedRefreshProvider;
                root.queuedRefreshProvider = "";
                const allProviders = root.queuedRefreshAllProviders;
                root.queuedRefreshAllProviders = false;
                Qt.callLater(function() { root.refresh(forced, provider, allProviders); });
            }
        }

        function exec(cmd) {
            // Connecting an already-connected source is a no-op, so a slow
            // command cannot pile up duplicate runs.
            if (connectedSources.indexOf(cmd) === -1)
                connectSource(cmd);
        }
    }

    // Separate source for fire-and-forget launches: sharing `reader` would feed
    // the terminal's output into consume() and clobber the report.
    Plasma5Support.DataSource {
        id: launcher
        engine: "executable"
        connectedSources: []
        onNewData: sourceName => disconnectSource(sourceName)
        function exec(cmd) {
            if (connectedSources.indexOf(cmd) === -1)
                connectSource(cmd);
        }
    }

    Plasma5Support.DataSource {
        id: sessionReader
        engine: "executable"
        connectedSources: []
        onNewData: (sourceName, data) => {
            disconnectSource(sourceName);
            const stdout = data["stdout"] || "";
            try {
                const result = JSON.parse(stdout);
                const changes = Array.isArray(result.changes) ? result.changes : [];
                if (changes.length > 0) {
                    const latest = changes[changes.length - 1];
                    if (latest.provider)
                        root.selectVendor(latest.provider);
                    if (latest.new_identity && !root.accountUnit)
                        root.activeAccountOverride = latest.new_identity;
                    root.refresh(true, latest.provider || "");
                }
            } catch (error) {
                // A failed local scan must never replace valid usage data.
            }
        }
        function scan() {
            const cmd = Logic.buildSessionCommand(Plasmoid.configuration.binaryPath, root.fetchTimeoutSecs);
            if (connectedSources.indexOf(cmd) === -1)
                connectSource(cmd);
        }
    }

    function currentCommand(options) {
        const opts = options || {};
        if (!opts.account && !opts.provider && !opts.allProviders && root.activeAccountOverride) {
            opts.account = root.activeAccountOverride;
        }
        return Logic.buildCommand(Plasmoid.configuration.binaryPath, root.fetchTimeoutSecs, opts);
    }

    function refresh(forced, provider, allProviders) {
        const opts = forced ? { refresh: true } : {};
        if (provider)
            opts.provider = provider;
        if (allProviders)
            opts.allProviders = true;
        if (root.activeAccountOverride && !provider && !allProviders) {
            opts.account = root.activeAccountOverride;
        }
        const cmd = root.currentCommand(opts);
        if (root.pendingCommand !== "") {
            root.refreshQueued = true;
            root.queuedRefreshForced = root.queuedRefreshForced || !!forced;
            if (provider)
                root.queuedRefreshProvider = provider;
            root.queuedRefreshAllProviders = root.queuedRefreshAllProviders || !!allProviders;
            return;
        }
        root.pendingCommand = cmd;
        watchdog.restart();
        reader.exec(cmd);
    }

    function switchAccount(label) {
        if (!label) return;
        root.activeAccountOverride = label;
        launcher.exec(Logic.buildAccountSwitchCommand(Plasmoid.configuration.binaryPath, label));
        root.refresh(true);
    }

    function simulateRenewal() {
        root.dismissedRenewalIds = [];
        Plasmoid.configuration.dismissedRenewals = [];
        launcher.exec(Logic.buildSimulateRenewalCommand(Plasmoid.configuration.binaryPath));
        simulateTimer.restart();
    }

    // Backstop only. timeout(1) in the spawned command is what bounds and kills
    // a hung binary (see buildArgv); this covers the remaining case where the
    // data engine never reports back at all. Fires after timeout(1) would have,
    // so the process-level kill is what the user normally sees, not this.
    Timer {
        id: watchdog
        interval: (root.fetchTimeoutSecs + Logic.TIMEOUT_KILL_GRACE_SECS + 5) * 1000
        repeat: false
        onTriggered: {
            if (root.pendingCommand === "")
                return;
            reader.disconnectSource(root.pendingCommand);
            root.pendingCommand = "";
            root.failure = i18n("ai-monitor took too long (>%1s)", root.fetchTimeoutSecs);
            if (root.refreshQueued) {
                const forced = root.queuedRefreshForced;
                root.refreshQueued = false;
                root.queuedRefreshForced = false;
                const provider = root.queuedRefreshProvider;
                root.queuedRefreshProvider = "";
                const allProviders = root.queuedRefreshAllProviders;
                root.queuedRefreshAllProviders = false;
                Qt.callLater(function() { root.refresh(forced, provider, allProviders); });
            }
        }
    }

    function consume(data) {
        const exitCode = data["exit code"];
        const stdout = data["stdout"] || "";
        const stderr = data["stderr"] || "";

        // timeout(1) killed it. Report that as a timeout rather than letting it
        // fall through to "invalid output", which is what a half-written stdout
        // would otherwise look like.
        if (exitCode === Logic.EXIT_TIMED_OUT || exitCode === Logic.EXIT_KILLED) {
            root.failure = i18n("ai-monitor took too long (>%1s)", root.fetchTimeoutSecs);
            return;
        }

        const parsed = Logic.parseReport(stdout);
        if (!parsed.ok) {
            // Keep the ORIGINAL output as the detail line. A bare "invalid
            // output" throws away the only actionable thing the user has. A
            // vendor-side failure never lands here — it arrives as an entry
            // with status "error" — so this really is a missing or broken
            // binary.
            const headline = exitCode !== 0
                ? (stderr.trim() ? i18n("ai-monitor failed") : i18n("ai-monitor exited with %1", exitCode))
                : i18n("invalid output");
            const detail = Logic.safeText(
                (exitCode !== 0 ? stderr.trim() : parsed.raw) || "", 300);
            root.failure = detail ? headline + "\n" + detail.substring(0, 300) : headline;
            return;
        }

        root.failure = "";
        root.report = parsed;
        const keptDismissed = Logic.pruneDismissed(root.dismissedRenewalIds, parsed.renewals);
        if (keptDismissed.length !== (root.dismissedRenewalIds || []).length) {
            root.dismissedRenewalIds = keptDismissed;
            Plasmoid.configuration.dismissedRenewals = keptDismissed;
        }
        if (parsed.account && parsed.account.label && !root.accountUnit) {
            root.activeAccountOverride = parsed.account.label;
        }
        if (parsed.primary && parsed.primary !== root.vendor) {
            const hasVendor = (parsed.entries || []).some(function(e) { return e.id === root.vendor; });
            if (!hasVendor) {
                root.selectVendor(parsed.primary);
            }
        }
    }

    function selectVendor(id) {
        if (id && id !== root.vendor)
            Plasmoid.configuration.vendor = id;
    }

    // Set by the compact representation while hovered, so the account being
    // read does not slide away under the pointer.
    property bool carouselPaused: false

    // The wheel steps the carousel or the pager tooltip when either is on,
    // the vendor ring otherwise.
    function scrollPanel(delta) {
        if ((root.carousel || root.panel.hover === "pager") && root.panel.items.length > 1)
            root.panelPosition += delta;
        else
            root.cycleVendor(delta);
    }

    function cycleVendor(delta) {
        // Scroll walks the ring the user configured; the tab strip in the popup
        // is the discoverable way to reach a vendor that is not in it.
        const next = Logic.nextVendor(Plasmoid.configuration.vendorRing, root.vendor, delta);
        root.selectVendor(next);
    }

    function launchTui() {
        launcher.exec(Logic.buildTuiCommand(Plasmoid.configuration.terminalCommand));
    }

    // One report covers every vendor, so switching only re-picks an entry that
    // is already in hand — no refetch, and the popup repaints instantly.
    Timer {
        interval: Math.max(root.minIntervalSecs, root.panel.refreshInterval) * 1000
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: root.refresh(false, "", root.accountUnit)
    }

    // Cheap local tick for the countdowns. Deliberately not a fetch.
    Timer {
        interval: 30000
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: root.nowMs = Date.now()
    }

    // Local credential scan only; it never launches the potentially slow
    // aggregate usage command unless a login/session actually changed.
    Timer {
        interval: 10000
        running: true
        repeat: true
        onTriggered: {
            sessionReader.scan();
        }
    }

    Timer {
        interval: root.panel.interval * 1000
        running: root.carousel && root.panel.items.length > 1 && !root.carouselPaused
        repeat: true
        onTriggered: root.panelPosition += 1
    }

    // One-shot timer after simulating renewals to pick up the test file
    Timer {
        id: simulateTimer
        interval: 350
        repeat: false
        onTriggered: root.refresh(false)
    }
}
