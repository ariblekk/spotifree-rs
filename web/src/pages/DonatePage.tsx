import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";

const SAWERIA = "https://saweria.co/ariharyanto";

const PAYMENTS = [
  "QRIS",
  "GoPay",
  "OVO",
  "DANA",
  "ShopeePay",
  "Bank transfer",
];

export default function DonatePage() {
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

      <section className="border border-t-[3px] border-t-primary">
        <div className="flex flex-col gap-6 px-6 py-7 sm:flex-row sm:items-center sm:justify-between sm:gap-8 max-sm:px-4.5 max-sm:py-5.5">
          <div className="min-w-0">
            <h2 className="mb-1.5 text-[22px] leading-tight font-bold tracking-[-0.01em]">
              Saweria
            </h2>
            <p className="m-0 max-w-[56ch] text-[15px] text-foreground/80">
              Open the link, pick an amount, pay. No signup, no minimum.
            </p>
            <ul className="mt-4 flex list-none flex-wrap gap-1.5 p-0">
              {PAYMENTS.map((label) => (
                <li
                  key={label}
                  className="bg-muted px-2 py-1 text-[13px] text-muted-foreground"
                >
                  {label}
                </li>
              ))}
            </ul>
          </div>

          <Button
            asChild
            size="lg"
            className="w-full flex-none px-6 font-semibold sm:w-auto"
          >
            <a href={SAWERIA} target="_blank" rel="noopener">
              Donate via Saweria →
            </a>
          </Button>
        </div>
      </section>

      <Alert className="mt-7 mb-0 border-l-2 border-l-primary bg-muted/40">
        <AlertDescription className="text-[15px] text-foreground/80">
          Donations are voluntary and buy nothing: the API stays free and
          unauthenticated either way. Nothing here is affiliated with Spotify.
        </AlertDescription>
      </Alert>
    </main>
  );
}
