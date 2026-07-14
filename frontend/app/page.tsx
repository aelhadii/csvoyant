export default function Home() {
  return (
    <main style={{ maxWidth: 720, margin: "4rem auto", padding: "0 1.5rem" }}>
      <h1 style={{ fontSize: "2rem", marginBottom: "0.5rem" }}>CSVoyant</h1>
      <p style={{ color: "#555", lineHeight: 1.6 }}>
        Submit a URL to a CSV file; a background worker fetches, infers a schema, and ingests it
        into ClickHouse, then renders an auto-generated dashboard.
      </p>
      <p style={{ color: "#888", marginTop: "2rem", fontSize: "0.9rem" }}>
        Scaffold skeleton (Prompt A). Auth, submit, jobs, and dashboard UIs arrive in Prompt E.
      </p>
    </main>
  );
}
