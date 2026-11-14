import { Navigate, Route, Routes, useLocation } from "react-router-dom";
import Layout from "@/components/Layout";
import ApiPage from "@/pages/ApiPage";
import ChartsPage from "@/pages/ChartsPage";
import DetailPage from "@/pages/DetailPage";
import DonatePage from "@/pages/DonatePage";
import SearchPage from "@/pages/SearchPage";

/// Old URLs are still linked from bookmarks and external notes. The Rust server
/// answers `/detail.html` with a `308`, this covers the Vite dev server.
const LEGACY: Record<string, string> = {
  "/index.html": "/",
  "/detail.html": "/detail",
  "/api.html": "/api",
  "/donate.html": "/donate",
};

function LegacyRedirect() {
  const { pathname, search } = useLocation();
  const target = LEGACY[pathname];
  return target ? <Navigate replace to={`${target}${search}`} /> : <NotFound />;
}

function NotFound() {
  return (
    <main className="mx-auto max-w-[1400px] px-6 pt-14 pb-22 max-sm:px-4 max-sm:pt-9">
      <h1 className="mb-2.5 text-[42px] leading-[1.1] font-bold tracking-[-0.03em] max-sm:text-[clamp(1.6rem,7vw,2.4rem)]">
        Not found
      </h1>
      <p className="max-w-[62ch] text-[17px] text-muted-foreground">
        No page here. Try the{" "}
        <a href="/" className="text-primary underline-offset-2 hover:underline">
          search
        </a>{" "}
        or the{" "}
        <a
          href="/api"
          className="text-primary underline-offset-2 hover:underline"
        >
          API notes
        </a>
        .
      </p>
    </main>
  );
}

export default function App() {
  return (
    <Layout>
      <Routes>
        <Route path="/" element={<SearchPage />} />
        <Route path="/charts" element={<ChartsPage />} />
        <Route path="/detail" element={<DetailPage />} />
        <Route path="/api" element={<ApiPage />} />
        <Route path="/donate" element={<DonatePage />} />
        <Route path="*" element={<LegacyRedirect />} />
      </Routes>
    </Layout>
  );
}
