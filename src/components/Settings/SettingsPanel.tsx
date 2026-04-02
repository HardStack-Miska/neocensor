import { useEffect, useState, useCallback } from 'react';
import { RefreshCw, Download, AlertCircle, CheckCircle, XCircle, ArrowUpCircle } from 'lucide-react';
import { listen } from '@tauri-apps/api/event';
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { useThemeStore } from '../../stores/themeStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { Toggle } from '../common/Toggle';
import { SettingsGroup, SettingsRow, SmallButton } from '../common/SettingsGroup';
import { MONO, SANS } from '../../lib/theme';
import { toast } from '../../stores/toastStore';
import * as api from '../../lib/tauri';

type DlStatus = 'idle' | 'downloading' | 'installed' | 'failed' | 'timeout';

export const SettingsPanel = () => {
  const T = useThemeStore((s) => s.theme);
  const { dark, toggle } = useThemeStore();
  const { settings, fetchSettings, updateSettings } = useSettingsStore();

  const [binaries, setBinaries] = useState<api.BinaryStatus | null>(null);
  const [versions, setVersions] = useState<api.ComponentVersions | null>(null);
  const [dlStatus, setDlStatus] = useState<DlStatus>('idle');
  const [checkingVersions, setCheckingVersions] = useState(false);
  const [portError, setPortError] = useState('');

  // Updater state
  type UpdateStatus = 'idle' | 'checking' | 'available' | 'downloading' | 'ready' | 'uptodate' | 'error';
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus>('idle');
  const [updateVersion, setUpdateVersion] = useState('');

  const checkForUpdates = async () => {
    setUpdateStatus('checking');
    try {
      const update = await check();
      if (update) {
        setUpdateVersion(update.version);
        setUpdateStatus('available');
      } else {
        setUpdateStatus('uptodate');
        toast.success('You are on the latest version');
      }
    } catch (e) {
      setUpdateStatus('error');
      toast.error(`Update check failed: ${e}`);
    }
  };

  const installUpdate = async () => {
    setUpdateStatus('downloading');
    try {
      const update = await check();
      if (!update) return;
      await update.downloadAndInstall();
      setUpdateStatus('ready');
      toast.success('Update installed. Restarting...');
      setTimeout(() => relaunch(), 1500);
    } catch (e) {
      setUpdateStatus('error');
      toast.error(`Update failed: ${e}`);
    }
  };

  // Local state for text inputs — save on blur, not every keystroke
  const [localSocks, setLocalSocks] = useState(String(settings.xray_socks_port));
  const [localHttp, setLocalHttp] = useState(String(settings.xray_http_port));
  const [localProxyDns, setLocalProxyDns] = useState(settings.dns.proxy_dns);
  const [localDirectDns, setLocalDirectDns] = useState(settings.dns.direct_dns);

  // Sync local state when settings load from backend
  useEffect(() => {
    setLocalSocks(String(settings.xray_socks_port));
    setLocalHttp(String(settings.xray_http_port));
    setLocalProxyDns(settings.dns.proxy_dns);
    setLocalDirectDns(settings.dns.direct_dns);
  }, [settings.xray_socks_port, settings.xray_http_port, settings.dns.proxy_dns, settings.dns.direct_dns]);

  useEffect(() => {
    fetchSettings();
    api.checkBinaries().then(setBinaries).catch(() => {});
  }, [fetchSettings]);

  // Listen for download progress events from backend
  useEffect(() => {
    let cancelled = false;
    let unlistenFn: (() => void) | null = null;
    listen<{ component: string; status: string; error?: string }>('download-progress', (event) => {
      if (cancelled) return;
      const { status, error } = event.payload;
      if (status === 'downloading') {
        setDlStatus('downloading');
      } else if (status === 'installed') {
        setDlStatus('installed');
        toast.success('xray-core installed successfully');
        api.checkBinaries().then(setBinaries).catch(() => {});
      } else if (status === 'failed') {
        setDlStatus('failed');
        toast.error(`Download failed: ${error || 'unknown error'}`);
      } else if (status === 'timeout') {
        setDlStatus('timeout');
        toast.error('Download timed out after 5 minutes');
      }
    }).then((fn) => {
      if (cancelled) { fn(); return; }
      unlistenFn = fn;
    });
    return () => { cancelled = true; unlistenFn?.(); };
  }, []);

  const handleCheckVersions = async () => {
    setCheckingVersions(true);
    try {
      const v = await api.checkLatestVersions();
      setVersions(v);
    } catch (e) {
      toast.error(`Failed to check versions: ${e}`);
    } finally {
      setCheckingVersions(false);
    }
  };

  const handleDownload = async () => {
    setDlStatus('downloading');
    try {
      await api.downloadComponents();
      const b = await api.checkBinaries();
      setBinaries(b);
      if (b.xray_installed) setDlStatus('installed');
    } catch (e) {
      setDlStatus('failed');
      toast.error(`Download failed: ${e}`);
    }
  };

  const validatePorts = useCallback((socks: number, http: number) => {
    if (socks < 1024 || socks > 65535) return 'SOCKS port out of range (1024-65535)';
    if (http < 1024 || http > 65535) return 'HTTP port out of range (1024-65535)';
    if (socks === http) return 'Ports must be different';
    return '';
  }, []);

  const updateSetting = <K extends keyof typeof settings>(
    key: K,
    value: (typeof settings)[K],
  ) => {
    const next = { ...settings, [key]: value };

    // Validate ports before saving
    if (key === 'xray_socks_port' || key === 'xray_http_port') {
      const err = validatePorts(
        key === 'xray_socks_port' ? (value as number) : next.xray_socks_port,
        key === 'xray_http_port' ? (value as number) : next.xray_http_port,
      );
      setPortError(err);
      if (err) return; // Don't save invalid settings
    }

    updateSettings(next);
  };

  const savePortsOnBlur = () => {
    const socks = parseInt(localSocks, 10) || settings.xray_socks_port;
    const http = parseInt(localHttp, 10) || settings.xray_http_port;
    const err = validatePorts(socks, http);
    setPortError(err);
    if (err) return;
    updateSettings({ ...settings, xray_socks_port: socks, xray_http_port: http });
  };

  const saveDnsOnBlur = () => {
    updateSettings({
      ...settings,
      dns: { proxy_dns: localProxyDns, direct_dns: localDirectDns },
    });
  };

  const portInputStyle = (hasError: boolean) => ({
    padding: '4px 8px',
    borderRadius: 5,
    border: `1px solid ${hasError ? T.er : T.brd}`,
    background: T.input,
    color: T.t0,
    fontSize: 11,
    width: 70,
    fontFamily: MONO,
    outline: 'none',
    textAlign: 'center' as const,
  });

  return (
    <div
      style={{
        flex: 1,
        overflowY: 'auto',
        padding: '18px 22px',
        fontFamily: SANS,
      }}
    >
      {/* General */}
      <SettingsGroup title="General">
        <SettingsRow label="Theme" description="Switch appearance">
          <div style={{ display: 'flex', alignItems: 'center', gap: 7 }}>
            <Toggle value={dark} onChange={toggle} />
            <span style={{ fontSize: 10.5, color: T.t2 }}>{dark ? 'Dark' : 'Light'}</span>
          </div>
        </SettingsRow>
        <SettingsRow label="Launch at startup" description="Start with system">
          <Toggle
            value={settings.auto_start}
            onChange={(v) => updateSetting('auto_start', v)}
          />
        </SettingsRow>
        <SettingsRow label="Minimize to tray" description="Keep in background" last>
          <Toggle
            value={settings.start_minimized}
            onChange={(v) => updateSetting('start_minimized', v)}
          />
        </SettingsRow>
      </SettingsGroup>

      {/* Connection */}
      <SettingsGroup title="Connection">
        <SettingsRow label="System proxy" description="Auto-configure Windows proxy">
          <Toggle
            value={settings.system_proxy}
            onChange={(v) => updateSetting('system_proxy', v)}
          />
        </SettingsRow>
        <SettingsRow label="Kill Switch" description="Block traffic if VPN drops">
          <Toggle
            value={settings.kill_switch}
            onChange={(v) => updateSetting('kill_switch', v)}
          />
        </SettingsRow>
        <SettingsRow label="Auto-connect" description="Connect on start">
          <Toggle
            value={settings.auto_connect}
            onChange={(v) => updateSetting('auto_connect', v)}
          />
        </SettingsRow>
        <SettingsRow label="SOCKS5 port" description="Local proxy port">
          <input
            type="number"
            min={1024}
            max={65535}
            value={localSocks}
            onChange={(e) => setLocalSocks(e.target.value)}
            onBlur={savePortsOnBlur}
            style={portInputStyle(portError.includes('SOCKS'))}
          />
        </SettingsRow>
        <SettingsRow label="HTTP port" description="HTTP proxy port" last>
          <div>
            <input
              type="number"
              min={1024}
              max={65535}
              value={localHttp}
              onChange={(e) => setLocalHttp(e.target.value)}
              onBlur={savePortsOnBlur}
              style={portInputStyle(portError.includes('HTTP') || portError.includes('different'))}
            />
            {portError && (
              <div style={{ fontSize: 9, color: T.er, marginTop: 3 }}>{portError}</div>
            )}
          </div>
        </SettingsRow>
      </SettingsGroup>

      {/* DNS */}
      <SettingsGroup title="DNS">
        <SettingsRow label="Domestic DNS" description="Local domains">
          <input
            value={localDirectDns}
            onChange={(e) => setLocalDirectDns(e.target.value)}
            onBlur={saveDnsOnBlur}
            style={{
              padding: '4px 8px',
              borderRadius: 5,
              border: `1px solid ${T.brd}`,
              background: T.input,
              color: T.t0,
              fontSize: 11,
              width: 150,
              fontFamily: MONO,
              outline: 'none',
            }}
          />
        </SettingsRow>
        <SettingsRow label="Foreign DNS" description="Blocked domains" last>
          <input
            value={localProxyDns}
            onChange={(e) => setLocalProxyDns(e.target.value)}
            onBlur={saveDnsOnBlur}
            style={{
              padding: '4px 8px',
              borderRadius: 5,
              border: `1px solid ${T.brd}`,
              background: T.input,
              color: T.t0,
              fontSize: 11,
              width: 150,
              fontFamily: MONO,
              outline: 'none',
            }}
          />
        </SettingsRow>
      </SettingsGroup>

      {/* Updates */}
      <SettingsGroup title="Updates">
        <SettingsRow label="App version" description="Current installed version" last={updateStatus === 'idle' || updateStatus === 'uptodate' || updateStatus === 'checking'}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
            <span style={{ fontSize: 10.5, fontFamily: MONO, color: T.t1 }}>v0.2.0</span>
            <SmallButton onClick={checkForUpdates} disabled={updateStatus === 'checking' || updateStatus === 'downloading'}>
              <RefreshCw size={10} style={updateStatus === 'checking' ? { animation: 'spin .7s linear infinite' } : {}} />
              {updateStatus === 'checking' ? 'Checking...' : 'Check'}
            </SmallButton>
          </div>
        </SettingsRow>
        {updateStatus === 'available' && (
          <SettingsRow label={`v${updateVersion} available`} description="New version ready to install" last>
            <SmallButton onClick={installUpdate}>
              <ArrowUpCircle size={10} />
              Update now
            </SmallButton>
          </SettingsRow>
        )}
        {updateStatus === 'downloading' && (
          <SettingsRow label="Updating..." description="Downloading and installing" last>
            <Download size={12} style={{ animation: 'spin .7s linear infinite', color: T.ac }} />
          </SettingsRow>
        )}
        {updateStatus === 'ready' && (
          <SettingsRow label="Restarting..." description="Update applied, restarting app" last>
            <CheckCircle size={12} style={{ color: T.ok }} />
          </SettingsRow>
        )}
        {updateStatus === 'error' && (
          <SettingsRow label="Update failed" description="Try again later" last>
            <SmallButton onClick={checkForUpdates}>
              <RefreshCw size={10} />
              Retry
            </SmallButton>
          </SettingsRow>
        )}
      </SettingsGroup>

      {/* Components */}
      <SettingsGroup title="Components">
        <SettingsRow label="xray-core" description="VLESS proxy engine">
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
            {binaries?.xray_installed ? (
              <span style={{ fontSize: 10.5, color: T.ok, fontFamily: MONO, display: 'flex', alignItems: 'center', gap: 3 }}>
                <CheckCircle size={10} />
                {versions?.xray_latest ? `v${versions.xray_latest}` : 'installed'}
              </span>
            ) : (
              <span style={{ fontSize: 10.5, color: T.er, display: 'flex', alignItems: 'center', gap: 3 }}>
                <AlertCircle size={10} /> missing
              </span>
            )}
            <SmallButton onClick={handleCheckVersions}>
              <RefreshCw size={10} style={checkingVersions ? { animation: 'spin .7s linear infinite' } : {}} />
              Check
            </SmallButton>
          </div>
        </SettingsRow>

        {binaries !== null && !binaries.xray_installed && (
          <SettingsRow label="Download" description="Install xray-core" last>
            <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
              {dlStatus === 'downloading' && (
                <span style={{ fontSize: 10, color: T.t2, display: 'flex', alignItems: 'center', gap: 4 }}>
                  <Download size={10} style={{ animation: 'spin .7s linear infinite' }} />
                  Downloading...
                </span>
              )}
              {dlStatus === 'failed' || dlStatus === 'timeout' ? (
                <SmallButton onClick={handleDownload}>
                  <XCircle size={10} style={{ color: T.er }} />
                  Retry
                </SmallButton>
              ) : dlStatus !== 'downloading' ? (
                <SmallButton onClick={handleDownload}>
                  <Download size={10} />
                  Download
                </SmallButton>
              ) : null}
            </div>
          </SettingsRow>
        )}
      </SettingsGroup>
    </div>
  );
};
