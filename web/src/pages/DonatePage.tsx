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
/// Plays once the bill settles. `youtube-nocookie` keeps the tracker out until
/// the video is actually played.
const THANKS_VIDEO = "https://www.youtube-nocookie.com/embed/2DFKIllyMOY?rel=0";
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

  const close = useCallback(() => setState({ kind: "idle" }), []);

  // Esc closes the dialog, like any other overlay.
  useEffect(() => {
    if (state.kind !== "bill") return;

    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [state.kind, close]);

  const amounts = info?.amounts?.length ? info.amounts : FALLBACK_AMOUNTS;
  const qrisReady = info?.qris !== false;
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
                  min={1}
                  max={max}
                  step={1}
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

      {state.kind === "bill" && (
        <QrisDialog bill={state.bill} paid={state.paid} onClose={close} />
      )}
    </main>
  );
}

/// Scannable bill in an overlay, so the page keeps its layout and the payer has
/// one thing on screen. The barcode line explains the odd total (base + unique
/// code) instead of letting it look like a bug.
function QrisDialog({
  bill,
  paid,
  onClose,
}: {
  bill: DonateBill;
  paid: boolean;
  onClose: () => void;
}) {
  const unique = bill.amount - bill.base_amount;

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Pembayaran QRIS"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4"
      onClick={onClose}
    >
      <div
        className="w-full max-w-[420px] border border-border bg-card p-6"
        onClick={(event) => event.stopPropagation()}
      >
        {paid ? (
          <div className="flex flex-col items-center gap-3 text-center">
            <span className="text-[24px] font-bold tracking-[-0.01em]">
              Terima kasih!
            </span>
            <span className="text-[15px] text-muted-foreground">
              Pembayaran {rupiah(bill.amount)} sudah diterima.
            </span>

            <div className="mt-1 aspect-video w-full border border-border bg-black">
              <iframe
                src={THANKS_VIDEO}
                title="Terima kasih"
                allow="accelerometer; autoplay; encrypted-media; gyroscope; picture-in-picture; fullscreen"
                allowFullScreen
                referrerPolicy="strict-origin-when-cross-origin"
                className="h-full w-full"
              />
            </div>

            <Button className="px-6 font-semibold" onClick={onClose}>
              Tutup
            </Button>
          </div>
        ) : (
          <div className="flex flex-col items-center gap-3">
            <h2 className="text-[20px] leading-tight font-bold tracking-[-0.01em]">
              Scan untuk bayar
            </h2>
            <p className="m-0 text-[14px] text-muted-foreground">
              Buka m-banking atau e-wallet, lalu pindai kode di bawah.
            </p>

            <div className="mt-1 bg-white p-3">
              <QRCodeSVG
                value={bill.qris_code}
                size={228}
                marginSize={0}
                bgColor="#ffffff"
                fgColor="#000000"
              />
            </div>

            <div className="w-full pt-1 text-center">
              <span className="block text-[13px] text-muted-foreground">
                Total yang harus dibayar
              </span>
              <span className="block text-[26px] font-bold tracking-[-0.01em]">
                {rupiah(bill.amount)}
              </span>
            </div>

            {unique > 0 && (
              <p className="m-0 w-full border border-border bg-muted/40 px-3 py-2 text-left text-[13px] leading-relaxed text-muted-foreground">
                Nominalnya sengaja berbeda {rupiah(unique)} dari{" "}
                {rupiah(bill.base_amount)}. Itu <strong>kode unik</strong> yang
                dipakai sistem untuk mencocokkan pembayaranmu secara otomatis —
                bukan biaya tambahan, dan ikut terbayar sebagai donasi. Bayar
                sesuai nominal di atas, jangan dibulatkan.
              </p>
            )}

            <span className="text-[12px] text-muted-foreground">
              {bill.trx_id} · menunggu pembayaran
            </span>

            <Button
              variant="outline"
              className="mt-1 w-full px-6 font-semibold"
              onClick={onClose}
            >
              Batal
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}
