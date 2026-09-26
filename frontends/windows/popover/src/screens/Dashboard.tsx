import MdiTune from "~icons/mdi/tune-variant";
import { HintCard } from "@/components/HintCard";
import { ErrorRow, ProviderSection } from "@/components/ProviderSection";
import type { RowAction } from "@/components/RowMenu";
import { UpdateBanner } from "@/components/UpdateBanner";
import { SortableItem, VerticalDnd } from "@/components/dnd";
import type { Card, Layout, Payload } from "@/lib/types";
import { explainError, updateBannerPending } from "../model.js";

interface DashboardProps {
  cards: Card[];
  hint: boolean;
  layout: Layout;
  nowMs: number;
  payload: Payload;
  visible: Card[];
  onCustomizeProvider: (id: string) => void;
  onDismissHint: () => void;
  onOpenCustomize: () => void;
  onReorder: (ids: string[]) => void;
  onResetProvider: (id: string) => void;
  onRowAction: (providerId: string, rowKey: string, action: RowAction) => void;
  onRowMenuOpenChange: (open: boolean) => void;
  onToggleCollapse: (id: string) => void;
  onToggleResetTimes: () => void;
  onToggleShowAs: () => void;
}

/** DashboardContentView: provider sections stacked with the density section gap. */
export function Dashboard({
  cards,
  hint,
  layout,
  nowMs,
  payload,
  visible,
  onCustomizeProvider,
  onDismissHint,
  onOpenCustomize,
  onReorder,
  onResetProvider,
  onRowAction,
  onRowMenuOpenChange,
  onToggleCollapse,
  onToggleResetTimes,
  onToggleShowAs,
}: DashboardProps) {
  if (payload.hostError) {
    return (
      <div className="card-surface py-[var(--card-gutter)]" title={payload.hostError}>
        <ErrorRow explained={explainError(payload.hostError)} />
      </div>
    );
  }
  const welcome = hint ? (
    <div className="mb-[var(--section-gap)]">
      <HintCard
        buttonTitle="Open Customize"
        icon={<MdiTune />}
        message="We turned on the providers that have credentials on this PC. Add or hide providers any time."
        title="Welcome to AI Usage"
        onAction={onOpenCustomize}
        onDismiss={onDismissHint}
      />
    </div>
  ) : null;
  const banner =
    payload.update && updateBannerPending(payload) ? (
      <div className="mb-[var(--section-gap)]">
        <UpdateBanner update={payload.update} />
      </div>
    ) : null;
  if (visible.length === 0) {
    return (
      <>
        {welcome}
        {banner}
        <p className="m-0 px-4 py-6 text-center text-[11px] text-label-2">
          {cards.length
            ? "Turn on Customize to choose what to show."
            : "No providers enabled. Open the TUI and turn one on in Settings."}
        </p>
      </>
    );
  }
  const ids = visible.map((card) => card.id);
  // Every account in use right now, like a click on any other frontend.
  const inUse =
    payload.inUse.length > 0 ? (
      <div className="card-surface mb-[var(--section-gap)] px-4 py-[var(--card-gutter)]">
        <p className="m-0 mb-1 text-[11px] font-semibold text-label-2">In use now</p>
        {payload.inUse.map((account) => (
          <div key={account.title} className="py-0.5 text-[12px]">
            <span className={account.active ? "font-semibold" : ""}>{account.title}</span>
            {account.providers.map((p) => (
              <span key={p.name} className="text-label-2">
                {"  ·  "}
                {p.name} {p.summary}
              </span>
            ))}
          </div>
        ))}
      </div>
    ) : null;
  return (
    <>
    {welcome}
    {banner}
    {inUse}
    <VerticalDnd
      items={ids}
      onReorder={onReorder}
      overlay={(id) => {
        const card = visible.find((item) => item.id === id);
        if (!card) return null;
        return <ProviderSection card={card} layout={layout} lifted nowMs={nowMs} />;
      }}
    >
      <div className="flex flex-col gap-[var(--section-gap)]">
        {visible.map((card) => (
          <SortableItem key={card.id} id={card.id}>
            {({ attributes, listeners }) => (
              <ProviderSection
                card={card}
                handle={{ attributes, listeners }}
                layout={layout}
                nowMs={nowMs}
                onCustomize={() => onCustomizeProvider(card.id)}
                onReset={() => onResetProvider(card.id)}
                onRowAction={(key, action) => onRowAction(card.id, key, action)}
                onRowMenuOpenChange={onRowMenuOpenChange}
                onToggleCollapse={() => onToggleCollapse(card.id)}
                onToggleResetTimes={onToggleResetTimes}
                onToggleShowAs={onToggleShowAs}
              />
            )}
          </SortableItem>
        ))}
      </div>
    </VerticalDnd>
    </>
  );
}
