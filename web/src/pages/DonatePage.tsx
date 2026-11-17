import { useCallback, useEffect, useRef, useState } from "react";
import { QRCodeSVG } from "qrcode.react";

import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  donateInfo,
  donateQris,
  donateStatus,
  type DonateBill,
  type DonateInfo,
} from "@/lib/api";

const SAWERIA = "https://saweria.co/ariharyanto";
const FALLBACK_AMOUNTS = [10_000, 25_000, 50_000, 100_000];
const POLL_MS = 3000;

function rupiah(value: number) {
  return new Intl.NumberFormat("id-ID", {
    style: "currency",
    currency: "IDR",
    maximumFractionDigits: 0,
  }).format(value);
}

type BillState =
  | { kind: "idle" }
  | { kind: "creating" }
  | { kind: "bill"; bill: DonateBill; paid: boolean }
  | { kind: "error"; message: string };

export default function DonatePage() {
  const [info, setInfo] = useState<DonateInfo | null>(null);
  const [amount, setAmount] = useState(FALLBACK_AMOUNTS[1]);
  const [state, setState] = useState<BillState>({ kind: "idle" });
  const timer = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    const controller = new AbortController();
    donateInfo(controller.signal)
      .then(setInfo)
      .catch(() => setInfo(null));
    return () => controller.abort();
  }, []);

  // Poll the bill until it settles; the gateway is the source of truth.
  useEffect(() => {
    if (state.kind !== "bill" || state.paid) return;

    const id = state.bill.qris_id;
    const controller = new AbortController();

    timer.current = setInterval(async () => {
      try {
        const status = await donateStatus(id, controller.signal);
        if (status.paid) {
          setState((current) =>
            current.kind === "bill" ? { ...current, paid: true } : current,
          );
        }
      } catch {
        // A dropped poll is harmless: the next tick retries.
      }
    }, POLL_MS);

    return () => {
      controller.abort();
      if (timer.current) clearInterval(timer.current);
      timer.current = null;
    };
  }, [state]);

  const create = useCallback(async () => {
    setState({ kind: "creating" });
    try {
      const bill = await donateQris(amount);
      setState({ kind: "bill", bill, paid: false });
    } catch (error) {
      setState({
        kind: "error",
        message: error instanceof Error ? error.message : "Gagal membuat QRIS",
      });
    }
  }, [amount]);

  const amounts = info?.amounts?.length ? info.amounts : FALLBACK_AMOUNTS;
  const qrisReady = info?.qris !== false;
  const min = info?.min ?? 1_000;
  const max = info?.max ?? 5_000_000;

  return (
    <main className="mx-auto max-w-[1400px] px-6 pt-14 pb-22 max-sm:px-4 max-sm:pt-9 max-sm:pb-15">
      <h1 className="mb-2.5 text-[42px] leading-[1.1] font-bold tracking-[-0.03em] max-sm:text-[clamp(1.6rem,7vw,2.4rem)]">
        Donate
      </h1>
      <p className="mb-9 max-w-[62ch] text-[17px] text-muted-foreground">
        No ads, no accounts, no tracking. Only the bill for a small server and
        the traffic it pulls from Spotify. If this saved you time, you can chip
        in.
      </p>

      <div className="flex flex-col gap-6 lg:flex-row">
        {qrisReady && (
          <section className="flex-1 border border-t-[3px] border-t-primary">
            <div className="px-6 py-7 max-sm:px-4.5 max-sm:py-5.5">
              <h2 className="mb-1.5 text-[22px] leading-tight font-bold tracking-[-0.01em]">
                QRIS
              </h2>
              <p className="m-0 max-w-[56ch] text-[15px] text-foreground/80">
                Pick an amount, scan the QR with any Indonesian bank or e-wallet
                app, and it settles instantly.
              </p>

              <div className="mt-4 flex flex-wrap gap-1.5">
                {amounts.map((value) => (
                  <Button
                    key={value}
                    size="sm"
                    variant={value === amount ? "default" : "outline"}
                    className="font-semibold"
                    onClick={() => setAmount(value)}
                  >
                    {rupiah(value)}
                  </Button>
                ))}
              </div>

              <label className="mt-4 flex max-w-[220px] flex-col gap-1.5 text-[13px] text-muted-foreground">
                Nominal lain
                <input
                  type="number"
                  inputMode="numeric"
                  min={min}
                  max={max}
                  step={1000}
                  value={amount}
                  onChange={(event) => setAmount(Number(event.target.value))}
                  className="h-9 border border-border bg-background px-2.5 text-[15px] text-foreground outline-none focus-visible:border-ring"
                />
              </label>

              <Button
                size="lg"
                className="mt-4 w-full px-6 font-semibold sm:w-auto"
                disabled={state.kind === "creating"}
                onClick={create}
              >
                {state.kind === "creating" ? "Membuat QR…" : "Buat QRIS →"}
              </Button>

              {state.kind === "error" && (
                <p className="mt-3 text-[14px] text-destructive">
                  {state.message}
                </p>
              )}
            </div>

            {state.kind === "bill" && (
              <div className="flex flex-col items-center gap-3 border-t border-border px-6 py-7 max-sm:px-4.5">
                {state.paid ? (
                  <div className="flex flex-col items-center gap-2 text-center">
                    <span className="text-[22px] font-bold tracking-[-0.01em]">
                      Terima kasih!
                    </span>
                    <span className="text-[15px] text-muted-foreground">
                      Pembayaran {rupiah(state.bill.amount)} sudah diterima.
                    </span>
                  </div>
                ) : (
                  <>
                    <div className="bg-white p-3">
                      <QRCodeSVG
                        value={state.bill.qris_code}
                        size={220}
                        marginSize={0}
                        bgColor="#ffffff"
                        fgColor="#000000"
                      />
                    </div>
                    <span className="text-[17px] font-semibold">
                      {rupiah(state.bill.amount)}
                    </span>
                    <span className="text-[13px] text-muted-foreground">
                      Trx {state.bill.trx_id} · menunggu pembayaran
                    </span>
                  </>
                )}
              </div>
            )}
          </section>
        )}

        <section className="flex-1 border border-t-[3px] border-t-primary">
          <div className="flex h-full flex-col gap-6 px-6 py-7 sm:flex-row sm:items-center sm:justify-between sm:gap-8 max-sm:px-4.5 max-sm:py-5.5">
            <div className="min-w-0">
              <h2 className="mb-1.5 text-[22px] leading-tight font-bold tracking-[-0.01em]">
                Saweria
              </h2>
              <p className="m-0 max-w-[56ch] text-[15px] text-foreground/80">
                Open the link, pick an amount, pay. No signup, no minimum.
              </p>
            </div>

            <Button
              asChild
              size="lg"
              variant="outline"
              className="w-full flex-none px-6 font-semibold sm:w-auto"
            >
              <a href={SAWERIA} target="_blank" rel="noopener">
                Donate via Saweria →
              </a>
            </Button>
          </div>
        </section>
      </div>

      <Alert className="mt-7 mb-0 border-l-2 border-l-primary bg-muted/40">
        <AlertDescription className="text-[15px] text-foreground/80">
          Donations are voluntary and buy nothing: the API stays free and
          unauthenticated either way. Nothing here is affiliated with Spotify.
        </AlertDescription>
      </Alert>
    </main>
  );
}
