import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import Database from "@tauri-apps/plugin-sql";

import "./App.css";
import QRCode from "qrcode.react";

function App() {
  const [serverStatus, setServerStatus] = useState<{
    isRunning: boolean;
    address: string | null;
  }>({
    isRunning: false,
    address: null,
  });
  const [statusMessage, setStatusMessage] = useState<string>("");
  const [error, setError] = useState<string | null>(null);

  const loadDb = async () => {
    await Database.load("sqlite:hikma-health.db");
  };

  // Get server status on component mount
  useEffect(() => {
    // on mount, also init the database:
    loadDb()
      .then(() => {
        console.log("Database loaded successfully");
      })
      .catch((err) => {
        alert(`Failed to load database: ${err}`);
      });
    checkServerStatus();
  }, []);

  // Function to check server status
  const checkServerStatus = async () => {
    try {
      const [isRunning, address] = await invoke<[boolean, string | null]>(
        "get_server_status"
      );
      setServerStatus({ isRunning, address });
    } catch (err) {
      setError(`Failed to get server status: ${err}`);
    }
  };

  // Start server function
  const startServer = async () => {
    try {
      setError(null);
      setStatusMessage("Starting server...");
      const result = await invoke<string>("start_server_command");
      setStatusMessage(result);
      await checkServerStatus();
    } catch (err) {
      setError(`Failed to start server: ${err}`);
    }
  };

  // Stop server function
  const stopServer = async () => {
    try {
      setError(null);
      setStatusMessage("Stopping server...");
      const result = await invoke<string>("stop_server_command");
      setStatusMessage(result);
      await checkServerStatus();
    } catch (err) {
      setError(`Failed to stop server: ${err}`);
    }
  };

  return (
    <main className="container">
      <h1>Hikma Health Local Server</h1>

      <div className="server-controls">
        <div className="status-display">
          <h2>Server Status</h2>
          <p>
            Status:{" "}
            <span className={serverStatus.isRunning ? "online" : "offline"}>
              {serverStatus.isRunning ? "Online" : "Offline"}
            </span>
          </p>
          {statusMessage && <p className="status-message">{statusMessage}</p>}
          {error && <p className="error-message">{error}</p>}
        </div>

        <div className="button-container">
          <button
            onClick={startServer}
            disabled={serverStatus.isRunning}
            className={serverStatus.isRunning ? "disabled" : "primary"}
          >
            Start Server
          </button>
          <button
            onClick={stopServer}
            disabled={!serverStatus.isRunning}
            className={!serverStatus.isRunning ? "disabled" : "danger"}
          >
            Stop Server
          </button>
          <button onClick={checkServerStatus} className="secondary">
            Refresh Status
          </button>
        </div>
      </div>

      {serverStatus.address && (
        <div className="qr-container">
          <h2>Server Address</h2>
          <p>{serverStatus.address}</p>
          <div className="qr-code">
            <QRCode value={serverStatus.address} size={256} />
          </div>
          <p className="help-text">
            Scan this QR code with your mobile device to connect to the server
          </p>
        </div>
      )}
    </main>
  );
}

export default App;
