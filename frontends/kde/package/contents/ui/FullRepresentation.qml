// The popup. Layout follows the native Omarchy panel (frontends/omarchy/Panel.qml) so the
// two native frontends read the same: header, provider tabs, a status surface
// when something is wrong, the usage rows under one heading, and a footer
// saying how old the data is.
//
// Only the STRUCTURE is ported. Omarchy draws with its shell's own theme
// tokens; everything here sizes off Kirigami.Units and colours off
// Kirigami.Theme, so the widget follows whatever Plasma colour scheme the user
// runs.
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import org.kde.plasma.components as PlasmaComponents
import "../code/plasmoid-logic.mjs" as Logic

Item {
    id: full

    required property var applet

    onVisibleChanged: {
        if (visible) {
            accountCombo.syncIndex();
        }
    }

    readonly property var entries: (full.applet.report && full.applet.report.entries) || []
    readonly property var entry: full.applet.entry
    readonly property string status: full.applet.statusMessage()
    readonly property var rows: Logic.detailRows(full.entry, full.applet.showExtraModels)
    // A click always lists every account in use right now, one card each with
    // the providers it is the current session for. Without accounts
    // (individual-session mode) the popup keeps the provider view.
    readonly property bool accountBlockMode: full.applet.accounts.length > 0

    readonly property bool hasRows: {
        const list = full.entries;
        for (let i = 0; i < list.length; i++) {
            const r = Logic.detailRows(list[i], full.applet.showExtraModels);
            if (r && r.length > 0) return true;
            const em = full.displayedExtraModelsFor(list[i]);
            if (em && em.length > 0) return true;
        }
        return false;
    }

    function sharedPoolPercentFor(e) {
        if (!e || !e.sections || e.id !== "antigravity") return null;
        for (const s of e.sections) {
            if (s.type === "metric" && /claude|gpt/i.test(s.label)) {
                return s.percent;
            }
        }
        return null;
    }

    function displayedExtraModelsFor(e) {
        if (!full.applet.showExtraModels || !e || !e.extraModels || e.id !== "antigravity")
            return [];
        const selected = Array.from(full.applet.selectedExtraModels || []);
        if (selected.length === 0)
            return e.extraModels;
        return e.extraModels.filter(m => selected.indexOf(m) !== -1);
    }

    function findAccountIndex(label) {
        const accs = full.applet.accounts || [];
        if (accs.length === 0) return 0;
        if (label) {
            for (let i = 0; i < accs.length; i++) {
                if (accs[i].label === label || accs[i].user === label)
                    return i;
            }
        }
        const entryId = full.entry ? full.entry.id : "";
        if (entryId) {
            for (let i = 0; i < accs.length; i++) {
                const provs = accs[i].providers || [];
                for (let j = 0; j < provs.length; j++) {
                    if (provs[j].id === entryId) return i;
                }
            }
        }
        return 0;
    }

    readonly property int activeDisplayMode: 0

    // An Item defaults to implicitHeight 0 and the popup sizes itself from the
    // implicit size, so without this the buttons render off-canvas. The
    // maximum is what makes the popup SHRINK again when a smaller vendor is
    // selected rather than keeping the tallest height it ever had.
    readonly property int contentHeight: column.implicitHeight + Kirigami.Units.largeSpacing * 2
    implicitWidth: Kirigami.Units.gridUnit * 22
    implicitHeight: full.contentHeight
    Layout.minimumHeight: full.contentHeight
    Layout.preferredHeight: full.contentHeight
    Layout.maximumHeight: full.contentHeight

    ColumnLayout {
        id: column
        // Deliberately not anchors.fill: the column must drive the height, not
        // be stretched by it, or contentHeight feeds back on itself.
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: Kirigami.Units.largeSpacing
        spacing: Kirigami.Units.smallSpacing

        // --- header --------------------------------------------------------
        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            // isMask is explicit on purpose: Kirigami only infers it for some
            // icons, and without it `color` is ignored and the full-colour
            // artwork renders — a little monitor with a chart painted on it,
            // which reads as a picture rather than as this widget's mark and
            // ignores the colour scheme entirely.
            Kirigami.Icon {
                source: "speedometer-symbolic"
                isMask: true
                implicitWidth: Kirigami.Units.iconSizes.medium
                implicitHeight: Kirigami.Units.iconSizes.medium
                color: full.applet.statusIsUrgent()
                    ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.textColor
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0

                Kirigami.Heading {
                    Layout.fillWidth: true
                    level: 3
                    elide: Text.ElideRight
                    text: {
                        if (full.entries.length > 1) {
                            return full.entries.map(function(e) { return e.label; }).join(" + ");
                        }
                        return full.entry ? full.entry.label : i18n("AI Monitor");
                    }
                    textFormat: Text.PlainText
                }

                PlasmaComponents.Label {
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    font: Kirigami.Theme.smallFont
                    opacity: 0.7
                    visible: text !== ""
                    text: {
                        if (full.entries.length > 1) {
                            return full.entries.map(function(e) {
                                return e.plan ? (e.label + ": " + e.plan) : e.label;
                            }).join(" · ");
                        }
                        if (!full.entry)
                            return "";
                        return full.entry.plan || "";
                    }
                    textFormat: Text.PlainText
                }
            }

            PlasmaComponents.ToolButton {
                icon.name: "view-refresh-symbolic"
                display: PlasmaComponents.AbstractButton.IconOnly
                enabled: full.applet.pendingCommand === ""
                text: i18n("Refresh now")
                PlasmaComponents.ToolTip.text: text
                PlasmaComponents.ToolTip.visible: hovered
                PlasmaComponents.ToolTip.delay: Kirigami.Units.toolTipDelay
                onClicked: {
                    full.applet.refresh(true, "", full.accountBlockMode);
                }
            }

            PlasmaComponents.ToolButton {
                icon.name: "dialog-information"
                display: PlasmaComponents.AbstractButton.IconOnly
                text: i18n("Simulate Renewal Alert")
                PlasmaComponents.ToolTip.text: text
                PlasmaComponents.ToolTip.visible: hovered
                PlasmaComponents.ToolTip.delay: Kirigami.Units.toolTipDelay
                onClicked: full.applet.simulateRenewal()
            }

            PlasmaComponents.ToolButton {
                icon.name: "utilities-terminal-symbolic"
                display: PlasmaComponents.AbstractButton.IconOnly
                text: i18n("Open TUI")
                PlasmaComponents.ToolTip.text: text
                PlasmaComponents.ToolTip.visible: hovered
                PlasmaComponents.ToolTip.delay: Kirigami.Units.toolTipDelay
                onClicked: full.applet.launchTui()
            }
        }


        // --- account selector ----------------------------------------------
        RowLayout {
            Layout.fillWidth: true
            visible: full.activeDisplayMode === 0 && !full.accountBlockMode
                && full.applet.accounts.length > 0
            spacing: Kirigami.Units.smallSpacing

            PlasmaComponents.Label {
                text: i18n("Account:")
                font: Kirigami.Theme.smallFont
                opacity: 0.7
                textFormat: Text.PlainText
            }

            QQC2.ComboBox {
                id: accountCombo
                Layout.fillWidth: true
                model: full.applet.accounts

                function syncIndex() {
                    var targetIdx = full.findAccountIndex(full.applet.currentAccountLabel);
                    if (accountCombo.currentIndex !== targetIdx) {
                        accountCombo.currentIndex = targetIdx;
                    }
                }

                onModelChanged: syncIndex()
                Component.onCompleted: syncIndex()

                displayText: {
                    const cur = full.applet.currentAccountLabel;
                    const acc = full.applet.accounts[currentIndex];
                    if (acc && (!cur || acc.label === cur || acc.user === cur)) {
                        return Logic.formatAccount(acc.label, full.applet.showFullEmail);
                    }
                    return cur ? Logic.formatAccount(cur, full.applet.showFullEmail) : (acc ? Logic.formatAccount(acc.label, full.applet.showFullEmail) : "");
                }

                delegate: QQC2.ItemDelegate {
                    required property var modelData
                    required property int index
                    width: accountCombo.width
                    text: Logic.formatAccount(modelData.label, full.applet.showFullEmail)
                    highlighted: accountCombo.highlightedIndex === index
                }

                onActivated: index => {
                    const acc = full.applet.accounts[index];
                    if (acc) full.applet.switchAccount(acc.label);
                }

                function cycleAccount(delta) {
                    if (!full.applet.accounts || full.applet.accounts.length <= 1) return;
                    var total = full.applet.accounts.length;
                    var curIdx = full.findAccountIndex(full.applet.currentAccountLabel);
                    var nextIdx = (curIdx + delta) % total;
                    if (nextIdx < 0) nextIdx += total;
                    const nextAcc = full.applet.accounts[nextIdx];
                    if (nextAcc) {
                        accountCombo.currentIndex = nextIdx;
                        full.applet.switchAccount(nextAcc.label);
                    }
                }

                MouseArea {
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    width: Kirigami.Units.gridUnit * 2
                    cursorShape: Qt.PointingHandCursor
                    z: 5
                    onClicked: mouse => {
                        if (mouse.y < height / 2) {
                            accountCombo.cycleAccount(-1);
                        } else {
                            accountCombo.cycleAccount(1);
                        }
                    }
                    onWheel: wheel => {
                        const d = wheel.angleDelta.y !== 0 ? wheel.angleDelta.y : wheel.angleDelta.x;
                        if (d > 0) {
                            accountCombo.cycleAccount(-1);
                        } else if (d < 0) {
                            accountCombo.cycleAccount(1);
                        }
                    }
                }
            }

            Connections {
                target: full.applet
                function onCurrentAccountLabelChanged() {
                    accountCombo.syncIndex();
                }
                function onAccountsChanged() {
                    accountCombo.syncIndex();
                }
                function onReportChanged() {
                    accountCombo.syncIndex();
                }
                function onEntryChanged() {
                    accountCombo.syncIndex();
                }
                function onVendorChanged() {
                    accountCombo.syncIndex();
                }
            }

            PlasmaComponents.ToolButton {
                icon.name: "go-up-symbolic"
                display: PlasmaComponents.AbstractButton.IconOnly
                enabled: full.applet.accounts.length > 1
                PlasmaComponents.ToolTip.text: i18n("Previous account")
                PlasmaComponents.ToolTip.visible: hovered
                PlasmaComponents.ToolTip.delay: Kirigami.Units.toolTipDelay
                onClicked: accountCombo.cycleAccount(-1)
            }

            PlasmaComponents.ToolButton {
                icon.name: "go-down-symbolic"
                display: PlasmaComponents.AbstractButton.IconOnly
                enabled: full.applet.accounts.length > 1
                PlasmaComponents.ToolTip.text: i18n("Next account")
                PlasmaComponents.ToolTip.visible: hovered
                PlasmaComponents.ToolTip.delay: Kirigami.Units.toolTipDelay
                onClicked: accountCombo.cycleAccount(1)
            }
        }

        // --- provider tabs omitted ---
        // When an account has multiple providers, all are displayed together below
        // without separating into tabs.

        // --- Renewal Notice Alert Banner / Card -----------------------------
        Rectangle {
            id: renewalCard
            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.smallSpacing
            visible: full.applet.activeRenewalGroups.length > 0
            implicitHeight: renewalColumn.implicitHeight + Kirigami.Units.smallSpacing * 2
            radius: Kirigami.Units.cornerRadius
            color: Qt.alpha(Kirigami.Theme.highlightColor, 0.08)
            border.width: 1
            border.color: Qt.alpha(Kirigami.Theme.highlightColor, 0.4)

            ColumnLayout {
                id: renewalColumn
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: Kirigami.Units.smallSpacing
                spacing: Kirigami.Units.smallSpacing

                // Header
                RowLayout {
                    Layout.fillWidth: true
                    spacing: Kirigami.Units.smallSpacing

                    Kirigami.Icon {
                        source: "notifications"
                        implicitWidth: Kirigami.Units.iconSizes.small
                        implicitHeight: Kirigami.Units.iconSizes.small
                        color: Kirigami.Theme.highlightColor
                        Layout.alignment: Qt.AlignVCenter
                    }

                    PlasmaComponents.Label {
                        Layout.fillWidth: true
                        font.bold: true
                        font.pointSize: Kirigami.Theme.defaultFont.pointSize
                        text: i18n("Cotas Renovadas (%1)", full.applet.activeRenewalGroups.length)
                        color: Kirigami.Theme.textColor
                        textFormat: Text.PlainText
                    }

                    PlasmaComponents.Button {
                        text: i18n("✕ Dispensar")
                        display: PlasmaComponents.AbstractButton.TextOnly
                        onClicked: full.applet.dismissActiveRenewals()
                    }
                }

                // Individual renewal notifications
                Repeater {
                    model: full.applet.activeRenewalGroups

                    delegate: Rectangle {
                        id: renewalItem
                        required property var modelData
                        Layout.fillWidth: true
                        radius: Kirigami.Units.cornerRadius
                        color: Qt.alpha(Kirigami.Theme.backgroundColor, 0.6)
                        border.width: 1
                        border.color: Qt.alpha(Kirigami.Theme.separatorColor || Kirigami.Theme.textColor, 0.15)
                        implicitHeight: itemLayout.implicitHeight + Kirigami.Units.smallSpacing * 2

                        RowLayout {
                            id: itemLayout
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.top: parent.top
                            anchors.margins: Kirigami.Units.smallSpacing
                            spacing: Kirigami.Units.smallSpacing

                            Kirigami.Icon {
                                source: "dialog-ok-apply"
                                implicitWidth: Kirigami.Units.iconSizes.small
                                implicitHeight: Kirigami.Units.iconSizes.small
                                color: Kirigami.Theme.positiveTextColor
                                Layout.alignment: Qt.AlignTop
                                Layout.topMargin: 2
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 2

                                // Account & Provider Badge Row
                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: Kirigami.Units.smallSpacing

                                    PlasmaComponents.Label {
                                        Layout.fillWidth: true
                                        font.bold: true
                                        wrapMode: Text.WrapAnywhere
                                    text: Logic.formatAccount(renewalItem.modelData.account_label, full.applet.showFullEmail)
                                        textFormat: Text.PlainText
                                        color: Kirigami.Theme.textColor
                                    }

                                    Rectangle {
                                        radius: 3
                                        color: Qt.alpha(Kirigami.Theme.highlightColor, 0.2)
                                        implicitWidth: provText.implicitWidth + 8
                                        implicitHeight: provText.implicitHeight + 4
                                        Layout.alignment: Qt.AlignRight

                                        PlasmaComponents.Label {
                                            id: provText
                                            anchors.centerIn: parent
                                            font.pointSize: Kirigami.Theme.smallFont.pointSize
                                            font.bold: true
                                            text: renewalItem.modelData.provider_name || renewalItem.modelData.provider_id || ""
                                            color: Kirigami.Theme.highlightColor
                                            textFormat: Text.PlainText
                                        }
                                    }
                                }

                                Repeater {
                                    model: renewalItem.modelData.renewals || []

                                    delegate: PlasmaComponents.Label {
                                        required property var modelData
                                        Layout.fillWidth: true
                                        wrapMode: Text.WordWrap
                                        font.pointSize: Kirigami.Theme.smallFont.pointSize
                                        text: i18n("✓ %1 renovada e pronta para uso — %2.",
                                            modelData.metric_label || i18n("Cota"),
                                            Logic.renewalWindowDescription(modelData))
                                        color: Kirigami.Theme.positiveTextColor
                                        textFormat: Text.PlainText
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // --- status surface -------------------------------------------------
        Rectangle {
            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.smallSpacing
            visible: full.activeDisplayMode === 0 && full.status !== ""
            implicitHeight: visible ? statusLabel.implicitHeight + Kirigami.Units.largeSpacing : 0
            radius: Kirigami.Units.cornerRadius
            color: Qt.alpha(full.applet.statusIsUrgent()
                ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.textColor, 0.09)
            border.width: 1
            border.color: Qt.alpha(full.applet.statusIsUrgent()
                ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.textColor, 0.35)

            PlasmaComponents.Label {
                id: statusLabel
                anchors.fill: parent
                anchors.margins: Kirigami.Units.smallSpacing
                wrapMode: Text.WordWrap
                font: Kirigami.Theme.smallFont
                text: full.status
                textFormat: Text.PlainText
            }
        }

        // --- usage rows (unified layout) ------------------------------------
        Kirigami.Separator {
            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.smallSpacing
            visible: full.activeDisplayMode === 0 && !full.accountBlockMode
                && full.applet.viewMode === 0 && full.hasRows
        }

        PlasmaComponents.Label {
            Layout.fillWidth: true
            visible: full.activeDisplayMode === 0 && !full.accountBlockMode
                && full.applet.viewMode === 0 && full.hasRows
            font: Kirigami.Theme.smallFont
            opacity: 0.6
            text: i18n("USAGE & BALANCE")
            textFormat: Text.PlainText
        }

        Repeater {
            model: full.activeDisplayMode === 0 && !full.accountBlockMode && full.applet.viewMode === 0
                ? full.entries
                : []

            delegate: ColumnLayout {
                id: entryGroup
                required property var modelData
                required property int index

                readonly property var entryRows: Logic.detailRows(modelData, full.applet.showExtraModels)
                readonly property var extraModels: full.displayedExtraModelsFor(modelData)
                readonly property var poolPct: full.sharedPoolPercentFor(modelData)

                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing
                visible: entryRows.length > 0 || extraModels.length > 0 || (modelData && (modelData.status === "error" || !!modelData.error))

                // If multiple entries, show a separator between providers and a provider header
                Kirigami.Separator {
                    Layout.fillWidth: true
                    Layout.topMargin: Kirigami.Units.smallSpacing
                    visible: full.entries.length > 1 && entryGroup.index > 0
                }

                RowLayout {
                    Layout.fillWidth: true
                    Layout.topMargin: Kirigami.Units.smallSpacing
                    visible: full.entries.length > 1
                    spacing: Kirigami.Units.smallSpacing

                    Kirigami.Heading {
                        level: 4
                        text: (entryGroup.modelData ? entryGroup.modelData.label : "").toUpperCase()
                        textFormat: Text.PlainText
                    }

                    PlasmaComponents.Label {
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        font: Kirigami.Theme.smallFont
                        opacity: 0.6
                        text: (entryGroup.modelData && entryGroup.modelData.plan) ? ("(" + entryGroup.modelData.plan + ")") : ""
                        textFormat: Text.PlainText
                    }
                }

                // Error banner if this specific entry errored
                Rectangle {
                    Layout.fillWidth: true
                    visible: entryGroup.modelData && (entryGroup.modelData.status === "error" || !!entryGroup.modelData.error)
                    implicitHeight: entryErrLabel.implicitHeight + Kirigami.Units.smallSpacing * 2
                    radius: Kirigami.Units.cornerRadius
                    color: Qt.alpha(Kirigami.Theme.negativeTextColor, 0.15)
                    border.width: 1
                    border.color: Kirigami.Theme.negativeTextColor

                    PlasmaComponents.Label {
                        id: entryErrLabel
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.margins: Kirigami.Units.smallSpacing
                        wrapMode: Text.WordWrap
                        color: Kirigami.Theme.negativeTextColor
                        text: Logic.errorMessage(entryGroup.modelData ? entryGroup.modelData.error : "") || i18n("Erro ao carregar provedor.")
                        textFormat: Text.PlainText
                    }
                }

                // Usage rows for this entry
                Repeater {
                    model: entryGroup.entryRows

                    UsageRow {
                        required property var modelData

                        Layout.fillWidth: true
                        row: modelData
                        colors: full.applet.colors
                        resetText: full.applet.resetText(modelData.resetAt)
                        showBar: true
                    }
                }

                // Extra models (Antigravity Pool) if present for this entry
                Kirigami.Separator {
                    Layout.fillWidth: true
                    Layout.topMargin: Kirigami.Units.smallSpacing
                    visible: entryGroup.extraModels.length > 0
                }

                PlasmaComponents.Label {
                    Layout.fillWidth: true
                    visible: entryGroup.extraModels.length > 0
                    font: Kirigami.Theme.smallFont
                    opacity: 0.6
                    text: i18n("AVAILABLE MODELS (ANTIGRAVITY POOL)")
                    textFormat: Text.PlainText
                }

                Repeater {
                    model: entryGroup.extraModels

                    delegate: RowLayout {
                        id: extraModelRow
                        required property string modelData
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing

                        readonly property var poolPct: entryGroup.poolPct
                        readonly property bool isExhausted: poolPct !== null && poolPct >= 100

                        Kirigami.Icon {
                            source: extraModelRow.isExhausted ? "dialog-warning" : "emblem-favorite-symbolic"
                            implicitWidth: Kirigami.Units.iconSizes.small
                            implicitHeight: Kirigami.Units.iconSizes.small
                            color: extraModelRow.isExhausted
                                ? Kirigami.Theme.negativeTextColor
                                : Kirigami.Theme.positiveTextColor
                        }

                        PlasmaComponents.Label {
                            Layout.fillWidth: true
                            text: "↳ " + extraModelRow.modelData
                            textFormat: Text.PlainText
                            font: Kirigami.Theme.smallFont
                        }

                        PlasmaComponents.Label {
                            text: {
                                if (extraModelRow.poolPct === null) return "";
                                if (extraModelRow.isExhausted) return i18n("Sem crédito (100%)");
                                return i18n("Compartilhado (%1%)", extraModelRow.poolPct);
                            }
                            textFormat: Text.PlainText
                            font: Kirigami.Theme.smallFont
                            color: extraModelRow.isExhausted
                                ? Kirigami.Theme.negativeTextColor
                                : (extraModelRow.poolPct >= 75 ? Kirigami.Theme.neutralTextColor : Kirigami.Theme.positiveTextColor)
                        }
                    }
                }
            }
        }

        PlasmaComponents.Label {
            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.smallSpacing
            visible: full.activeDisplayMode === 0 && !full.accountBlockMode
                && full.entries.length === 0 && full.status === "" && full.applet.viewMode === 0
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            opacity: 0.6
            text: i18n("No configured provider reported usage.")
            textFormat: Text.PlainText
        }

        // --- cards (alternative layout) -------------------------------------
        // The #142 card design, reworked: one card per entry the report
        // returned, gauges per window, failures and staleness inline. Sourced
        // from the same aggregate report as the tab view, so switching the
        // layout never refetches.
        VendorCards {
            Layout.fillWidth: true
            visible: full.activeDisplayMode === 0 && !full.accountBlockMode && full.applet.viewMode === 1
            applet: full.applet
        }

        AccountCards {
            Layout.fillWidth: true
            visible: full.activeDisplayMode === 0 && full.accountBlockMode
            applet: full.applet
        }

        // --- footer ---------------------------------------------------------
        PlasmaComponents.Label {
            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.smallSpacing
            visible: text !== ""
            horizontalAlignment: Text.AlignHCenter
            font: Kirigami.Theme.smallFont
            opacity: 0.6
            text: full.applet.updatedText()
            textFormat: Text.PlainText
        }
    }
}
