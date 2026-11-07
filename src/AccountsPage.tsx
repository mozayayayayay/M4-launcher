import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Plus, Trash2, UserRound, Check } from "lucide-react";

type LocalAccount = {
  id: string;
  username: string;
  accountType: string;
};

type AccountResponse = {
  accounts: LocalAccount[];
  selectedAccount: string | null;
};

type Props = {
  onNotice: (message: string) => void;
};

export default function AccountsPage({ onNotice }: Props) {
  const [accounts, setAccounts] = useState<LocalAccount[]>([]);
  const [selectedAccount, setSelectedAccount] = useState<string | null>(null);
  const [username, setUsername] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);

  async function refreshAccounts() {
    const result = await invoke<AccountResponse>("get_accounts");
    setAccounts(result.accounts ?? []);
    setSelectedAccount(result.selectedAccount ?? null);
  }

  useEffect(() => {
    let active = true;

    invoke<AccountResponse>("get_accounts")
      .then((result) => {
        if (!active) return;
        setAccounts(result.accounts ?? []);
        setSelectedAccount(result.selectedAccount ?? null);
      })
      .catch((error) => {
        if (active) {
          onNotice(`Couldn't load accounts: ${String(error)}`);
        }
      })
      .finally(() => {
        if (active) setLoading(false);
      });

    return () => {
      active = false;
    };
  }, []);

  async function addAccount() {
    const cleanUsername = username.trim();

    if (!/^[A-Za-z0-9_]{3,16}$/.test(cleanUsername) || busy) {
      return;
    }

    setBusy(true);

    try {
      const result = await invoke<AccountResponse>(
        "add_offline_account",
        { username: cleanUsername }
      );

      setAccounts(result.accounts ?? []);
      setSelectedAccount(result.selectedAccount ?? null);
      setUsername("");
      onNotice(`Added offline profile "${cleanUsername}".`);
    } catch (error) {
      onNotice(`Couldn't add account: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }

  async function selectAccount(id: string) {
    if (busy) return;

    setBusy(true);

    try {
      const result = await invoke<AccountResponse>("select_account", { id });
      setAccounts(result.accounts ?? []);
      setSelectedAccount(result.selectedAccount ?? null);
      onNotice("Active offline profile changed.");
    } catch (error) {
      onNotice(`Couldn't select account: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }

  async function deleteAccount(account: LocalAccount) {
    if (busy) return;

    if (!window.confirm(`Delete the local profile "${account.username}"?`)) {
      return;
    }

    setBusy(true);

    try {
      const result = await invoke<AccountResponse>("delete_account", {
        id: account.id
      });

      setAccounts(result.accounts ?? []);
      setSelectedAccount(result.selectedAccount ?? null);
      onNotice(`Deleted "${account.username}".`);
    } catch (error) {
      onNotice(`Couldn't delete account: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }

  const validUsername = /^[A-Za-z0-9_]{3,16}$/.test(username.trim());

  return (
    <div className="accounts-page">
      <div className="page-heading">
        <div>
          <div className="eyebrow">PROFILES</div>
          <h1>Accounts</h1>
          <p>Manage the local profiles used by your launcher.</p>
        </div>
      </div>

      <div className="section-title">
        <div>
          <h3>Add an offline profile</h3>
          <p>Use a Minecraft username between 3 and 16 characters.</p>
        </div>
      </div>

      <div className="info-card settings-card">
        <label>
          Username
          <input
            value={username}
            onChange={(event) => setUsername(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && validUsername) {
                void addAccount();
              }
            }}
            placeholder="YourMinecraftName"
            maxLength={16}
          />
        </label>

        <button
          className="primary-button"
          style={{ marginTop: 14 }}
          onClick={addAccount}
          disabled={!validUsername || busy}
        >
          <Plus size={16} />
          Add offline profile
        </button>

        {username.length > 0 && !validUsername && (
          <small style={{ display: "block", marginTop: 10 }}>
            Use 3–16 letters, numbers, or underscores.
          </small>
        )}
      </div>

      <div className="section-title">
        <div>
          <h3>Your profiles</h3>
          <p>{accounts.length} local profile(s).</p>
        </div>
      </div>

      {loading ? (
        <div className="info-card">Loading profiles...</div>
      ) : accounts.length === 0 ? (
        <div className="info-card">
          <div className="placeholder-icon">
            <UserRound size={22} />
          </div>
          <strong>No profiles yet</strong>
          <p>Add an offline profile to get started.</p>
        </div>
      ) : (
        <div className="account-list">
          {accounts.map((account) => {
            const active = selectedAccount === account.id;

            return (
              <div
                className="info-card"
                key={account.id}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 14,
                  marginBottom: 12
                }}
              >
                <div className="info-icon">
                  <UserRound size={20} />
                </div>

                <div style={{ minWidth: 0, flex: 1 }}>
                  <strong>{account.username}</strong>
                  <small style={{ display: "block" }}>
                    {account.accountType === "offline"
                      ? "Offline profile"
                      : account.accountType}
                    {active ? " · Active" : ""}
                  </small>
                </div>

                {active ? (
                  <span title="Active profile">
                    <Check size={18} />
                  </span>
                ) : (
                  <button
                    className="icon-button"
                    title="Select profile"
                    onClick={() => selectAccount(account.id)}
                    disabled={busy}
                  >
                    <Check size={17} />
                  </button>
                )}

                <button
                  className="icon-button"
                  title="Delete profile"
                  onClick={() => deleteAccount(account)}
                  disabled={busy}
                >
                  <Trash2 size={16} />
                </button>
              </div>
            );
          })}
        </div>
      )}

      <div className="modal-note" style={{ marginTop: 18 }}>
        Offline profiles are local launcher profiles. They do not sign in to
        Microsoft services or provide access to online-mode servers that
        require authenticated accounts.
      </div>
    </div>
  );
}
