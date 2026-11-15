import { Link, Navigate, Route, Routes, useLocation } from "react-router-dom";
import Layout from "@/components/Layout";
import ChartsPage from "@/pages/ChartsPage";
import DetailPage from "@/pages/DetailPage";
import DocsPage from "@/pages/DocsPage";
import DonatePage from "@/pages/DonatePage";
import SearchPage from "@/pages/SearchPage";

/// Old URLs are still linked from bookmarks and external notes. The Rust server
/// answers `/detail.html` with a `308`, this covers the Vite dev server.
const LEGACY: Record<string, string> = {
  "/index.html": "/",
  "/detail.html": "/detail",
  "/api.html": "/docs",
  "/api": "/docs",
  "/docs.html": "/docs",
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
        <Link to="/" className="text-primary underline-offset-2 hover:underline">
          search
        </Link>{" "}
        or the{" "}
        <Link
          to="/docs"
          className="text-primary underline-offset-2 hover:underline"
        >
          API docs
        </Link>
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
        <Route path="/docs" element={<DocsPage />} />
        <Route path="/api" element={<Navigate replace to="/docs" />} />
        <Route path="/donate" element={<DonatePage />} />
        <Route path="*" element={<LegacyRedirect />} />
      </Routes>
    </Layout>
  );
}
