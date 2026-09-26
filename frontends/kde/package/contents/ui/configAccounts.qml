import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.plasma.plasma5support as Plasma5Support
import org.kde.kirigami as Kirigami
import org.kde.kcmutils as KCM
import "../code/plasmoid-logic.mjs" as Logic

// Settings that live in config.toml, not in this applet: the panel layout
// ([display]) and the account and notification preferences in [ui]. Every
// frontend reads the same file, so a change here also changes Waybar, GNOME,
// Omarchy, the tray, macOS and the TUI. They are read with `settings show`
// and written with `settings apply` when the dialog's Apply is pressed.
KCM.SimpleKCM {
    id: page

    // Read-only here; configGeneral.qml owns them. Plain properties, not
    // aliases to controls, so this page never writes a changed value back.
    property string cfg_binaryPath: ""
    property int cfg_commandTimeout: 600

    // The one applet-local value on this page: which Antigravity extra
    // models the popup lists. The binary has no equivalent.
    property var cfg_selectedExtraModels: []

    // The loaded snapshot and the edits made since, as a settings patch.
    property var shared: null
    property var patch: ({})
    property var display: ({})
    property var accountsList: []
    property bool probing: true
    property string statusMessage: ""
    // Plasma enables Apply from this and calls saveConfig() on Apply.
    property bool unsavedChanges: false

    function value(key, fallback) {
        if (page.patch[key] !== undefined) return page.patch[key];
        return page.shared && page.shared[key] !== undefined ? page.shared[key] : fallback;
    }

    function set(key, v) {
        const next = Object.assign({}, page.patch);
        next[key] = v;
        page.patch = next;
        page.unsavedChanges = true;
    }

    function setDisplay(key, v) {
        const d = Object.assign({}, page.display);
        d[key] = v;
        page.display = d;
        const next = Object.assign({}, page.patch);
        next.display = Object.assign({}, next.display || {});
        next.display[key] = v;
        page.patch = next;
        page.unsavedChanges = true;
    }

    function isShownInPanel(label) {
        return (page.display.hidden_accounts || []).indexOf(label) === -1;
    }

    function setShownInPanel(label, on) {
        const hidden = (page.display.hidden_accounts || []).filter(l => l !== label);
        if (!on)
            hidden.push(label);
        page.setDisplay("hidden_accounts", hidden);
    }

    function hasExtraModel(modelName) {
        return Array.from(page.cfg_selectedExtraModels || []).indexOf(modelName) !== -1;
    }

    function toggleExtraModel(modelName, on) {
        const list = Array.from(page.cfg_selectedExtraModels || []).filter(m => m !== modelName);
        if (on) list.push(modelName);
        page.cfg_selectedExtraModels = list;
    }

    function saveConfig() {
        if (Object.keys(page.patch).length === 0)
            return;
        runner.exec(Logic.buildSettingsApplyCommand(page.cfg_binaryPath, page.patch));
        page.patch = ({});
        page.unsavedChanges = false;
    }

    Plasma5Support.DataSource {
        id: settingsReader
        engine: "executable"
        connectedSources: []
        onNewData: (sourceName, data) => {
            disconnectSource(sourceName);
            try {
                page.shared = JSON.parse(data["stdout"] || "");
                page.display = Object.assign({}, page.shared.display || {});
            } catch (error) {
                page.statusMessage = i18n("Could not read settings from ai-monitor.");
            }
        }
    }

    Plasma5Support.DataSource {
        id: prober
        engine: "executable"
        connectedSources: []
        onNewData: (sourceName, data) => {
            disconnectSource(sourceName);
            page.probing = false;
            page.accountsList = Logic.parseReport(data["stdout"] || "").accounts || [];
        }
    }

    Plasma5Support.DataSource {
        id: runner
        engine: "executable"
        connectedSources: []
        onNewData: (sourceName, data) => {
            disconnectSource(sourceName);
            if (data["exit code"] !== 0) {
                page.statusMessage = Logic.safeText(data["stderr"] || i18n("ai-monitor failed"), 200);
                return;
            }
            refreshAll();
        }
        function exec(cmd) {
            if (connectedSources.indexOf(cmd) === -1)
                connectSource(cmd);
        }
    }

    function refreshAll() {
        page.probing = true;
        settingsReader.connectSource(Logic.buildSettingsShowCommand(page.cfg_binaryPath));
        prober.connectSource(Logic.buildCommand(page.cfg_binaryPath, page.cfg_commandTimeout, { refresh: true }));
    }

    function removeAccount(label) {
        if (!label) return;
        page.statusMessage = i18n("Removing account %1…", label);
        runner.exec(Logic.buildAccountRemoveCommand(page.cfg_binaryPath, label));
    }

    Component.onCompleted: refreshAll()

    Kirigami.FormLayout {
        anchors.fill: parent

        QQC2.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            text: i18n("Saved in config.toml and shared by every ai-monitor frontend.")
            font: Kirigami.Theme.smallFont
            opacity: 0.7
            wrapMode: Text.WordWrap
            textFormat: Text.PlainText
        }

        QQC2.Label {
            visible: page.statusMessage !== ""
            text: page.statusMessage
            color: Kirigami.Theme.neutralTextColor
            font: Kirigami.Theme.smallFont
            wrapMode: Text.WordWrap
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            textFormat: Text.PlainText
        }

        // --- panel layout ---------------------------------------------------
        Item { Kirigami.FormData.isSection: true; Kirigami.FormData.label: i18n("Panel") }

        QQC2.ComboBox {
            Kirigami.FormData.label: i18n("Bar mode:")
            enabled: page.shared !== null
            model: [i18n("Expanded — items side by side"), i18n("Carousel — one at a time, rotating")]
            currentIndex: page.display.bar_mode === "carousel" ? 1 : 0
            onActivated: page.setDisplay("bar_mode", currentIndex === 1 ? "carousel" : "expanded")
        }

        QQC2.ComboBox {
            Kirigami.FormData.label: i18n("Each item is:")
            enabled: page.shared !== null && page.value("multi_account", true)
            model: [i18n("A provider (Claude, Gemini, Codex…)"), i18n("An account, with its providers")]
            currentIndex: page.display.bar_unit === "account" ? 1 : 0
            onActivated: page.setDisplay("bar_unit", currentIndex === 1 ? "account" : "provider")
        }

        QQC2.SpinBox {
            Kirigami.FormData.label: i18n("Items side by side:")
            visible: page.display.bar_mode !== "carousel"
            from: 1
            to: 6
            value: page.display.bar_count || 3
            onValueModified: page.setDisplay("bar_count", value)
        }

        QQC2.SpinBox {
            Kirigami.FormData.label: i18n("Switch item every:")
            visible: page.display.bar_mode === "carousel"
            from: 2
            to: 300
            value: page.display.carousel_interval || 5
            textFromValue: value => i18n("%1 s", value)
            valueFromText: text => {
                const n = parseInt(text, 10);
                return isNaN(n) ? 5 : n;
            }
            onValueModified: page.setDisplay("carousel_interval", value)
        }

        QQC2.ComboBox {
            Kirigami.FormData.label: i18n("On hover:")
            model: [i18n("Blocks — every item at once"), i18n("Pager — one item, scroll for the next")]
            currentIndex: page.display.hover_mode === "pager" ? 1 : 0
            onActivated: page.setDisplay("hover_mode", currentIndex === 1 ? "pager" : "blocks")
        }

        QQC2.CheckBox {
            visible: page.display.bar_unit === "account"
            text: i18n("Show the account name on the bar (off: only provider and usage)")
            checked: page.display.show_account_name !== false
            onToggled: page.setDisplay("show_account_name", checked)
        }

        QQC2.CheckBox {
            visible: page.display.bar_unit === "account"
            text: i18n("Show only each account's most recently used providers")
            checked: page.display.recent_only === true
            onToggled: page.setDisplay("recent_only", checked)
        }

        QQC2.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            text: i18n("Only accounts in use appear on the bar; the others are still monitored for renewals. Clicking the panel lists every account in use.")
            font: Kirigami.Theme.smallFont
            opacity: 0.7
            wrapMode: Text.WordWrap
            textFormat: Text.PlainText
        }

        // --- general --------------------------------------------------------
        Item { Kirigami.FormData.isSection: true; Kirigami.FormData.label: i18n("Accounts & updates") }

        QQC2.ComboBox {
            Kirigami.FormData.label: i18n("Account mode:")
            enabled: page.shared !== null
            model: [
                i18n("Multiple accounts (with account switcher)"),
                i18n("Individual session (active session only)")
            ]
            currentIndex: page.value("multi_account", true) ? 0 : 1
            onActivated: {
                page.set("multi_account", currentIndex === 0);
                if (currentIndex !== 0)
                    page.setDisplay("bar_unit", "provider");
            }
        }

        QQC2.CheckBox {
            text: i18n("Show full email address (off = username only)")
            checked: page.value("show_full_email", true)
            onToggled: page.set("show_full_email", checked)
        }

        QQC2.SpinBox {
            Kirigami.FormData.label: i18n("Refresh every:")
            from: 5
            to: 3600
            stepSize: 10
            value: page.value("refresh_interval", 300)
            textFromValue: value => value >= 60 && value % 60 === 0
                ? i18n("%1 s (%2 min)", value, value / 60) : i18n("%1 s", value)
            valueFromText: text => {
                const n = parseInt(text, 10);
                return isNaN(n) ? 300 : n;
            }
            onValueModified: page.set("refresh_interval", value)
        }

        RowLayout {
            Kirigami.FormData.label: i18n("Notifications:")
            spacing: Kirigami.Units.smallSpacing

            QQC2.CheckBox {
                text: i18n("When a 5h or weekly quota renews")
                checked: page.value("notify_resets", true)
                onToggled: page.set("notify_resets", checked)
            }

            QQC2.Button {
                text: i18n("Test")
                icon.name: "preferences-system-notifications"
                onClicked: runner.exec(Logic.buildMonitorTestCommand(page.cfg_binaryPath))
            }

            QQC2.Button {
                text: i18n("Simulate renewal")
                icon.name: "dialog-information"
                onClicked: runner.exec(Logic.buildSimulateRenewalCommand(page.cfg_binaryPath))
            }
        }

        QQC2.CheckBox {
            id: extraModelsCheck
            Kirigami.FormData.label: i18n("Extra models:")
            text: i18n("Show extra models (Claude & GPT in Antigravity)")
            checked: page.value("show_extra_models", false)
            onToggled: page.set("show_extra_models", checked)
        }

        Repeater {
            model: extraModelsCheck.checked ? [
                "Claude Sonnet 4.6 (Thinking)",
                "Claude Opus 4.6 (Thinking)",
                "GPT-OSS 120B (Medium)"
            ] : []

            delegate: QQC2.CheckBox {
                required property string modelData
                text: modelData + " — " + i18n("shared quota pool")
                checked: page.hasExtraModel(modelData)
                onToggled: page.toggleExtraModel(modelData, checked)
            }
        }

        // --- accounts -------------------------------------------------------
        Item { Kirigami.FormData.isSection: true; Kirigami.FormData.label: i18n("Monitored accounts") }

        QQC2.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            text: page.probing
                ? i18n("Checking accounts…")
                : (page.accountsList.length === 0
                    ? i18n("No monitored accounts found. Accounts appear as you log in.")
                    : i18n("Tick the accounts the panel shows when each item is an account. Remove any you no longer use."))
            font: Kirigami.Theme.smallFont
            opacity: 0.7
            wrapMode: Text.WordWrap
            textFormat: Text.PlainText
        }

        Repeater {
            model: page.accountsList

            delegate: Kirigami.AbstractCard {
                id: accountCard
                required property var modelData
                Layout.fillWidth: true
                Layout.maximumWidth: Kirigami.Units.gridUnit * 26

                contentItem: RowLayout {
                    spacing: Kirigami.Units.largeSpacing

                    QQC2.CheckBox {
                        enabled: page.display.bar_unit === "account"
                        checked: page.isShownInPanel(accountCard.modelData.label)
                        onToggled: page.setShownInPanel(accountCard.modelData.label, checked)
                        QQC2.ToolTip.text: i18n("Show this account in the panel")
                        QQC2.ToolTip.visible: hovered
                        Layout.alignment: Qt.AlignVCenter
                    }

                    ColumnLayout {
                        spacing: Kirigami.Units.smallSpacing / 2
                        Layout.fillWidth: true
                        Layout.alignment: Qt.AlignVCenter

                        RowLayout {
                            spacing: Kirigami.Units.smallSpacing

                            QQC2.Label {
                                text: Logic.formatAccount(accountCard.modelData.label, page.value("show_full_email", true))
                                font.bold: true
                                textFormat: Text.PlainText
                            }

                            QQC2.Label {
                                visible: accountCard.modelData.active
                                text: i18n("(Active)")
                                color: Kirigami.Theme.positiveTextColor
                                font: Kirigami.Theme.smallFont
                                textFormat: Text.PlainText
                            }
                        }

                        QQC2.Label {
                            text: {
                                const provs = accountCard.modelData.providers || [];
                                if (provs.length === 0) return i18n("No providers configured");
                                return i18n("Providers: %1", provs.map(p => p.name || p.id).join(", "));
                            }
                            font: Kirigami.Theme.smallFont
                            opacity: 0.65
                            textFormat: Text.PlainText
                        }
                    }

                    QQC2.Button {
                        icon.name: "edit-delete"
                        display: QQC2.AbstractButton.IconOnly
                        text: i18n("Remove account")
                        QQC2.ToolTip.text: i18n("Remove account %1", accountCard.modelData.label)
                        QQC2.ToolTip.visible: hovered
                        Layout.alignment: Qt.AlignVCenter
                        onClicked: page.removeAccount(accountCard.modelData.label)
                    }
                }
            }
        }
    }
}
