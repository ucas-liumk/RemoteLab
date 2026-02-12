import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  X,
  Upload,
  Download,
  FolderPlus,
  RefreshCw,
  Trash2,
  Folder,
  FileText,
  ChevronRight,
  Home,
  ArrowUp,
  Loader2,
  AlertCircle,
  FileArchive,
  FileImage,
  FileCode,
  File,
} from "lucide-react";
import * as api from "../services/api";
import type { RemoteFile } from "../services/types";

interface FileManagerProps {
  deviceName: string;
  host: string;
  user: string;
  port?: number;
  onClose: () => void;
}

function formatSize(bytes: number): string {
  if (bytes === 0) return "-";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return (bytes / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0) + " " + units[i];
}

function fileIcon(name: string, isDir: boolean) {
  if (isDir) return <Folder className="w-4 h-4 text-yellow-400" />;
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  if (["tar", "gz", "zip", "7z", "rar", "bz2", "xz"].includes(ext))
    return <FileArchive className="w-4 h-4 text-orange-400" />;
  if (["png", "jpg", "jpeg", "gif", "svg", "bmp", "webp", "ico"].includes(ext))
    return <FileImage className="w-4 h-4 text-pink-400" />;
  if (["py", "rs", "ts", "tsx", "js", "jsx", "c", "cpp", "h", "go", "java", "sh", "toml", "yaml", "yml", "json", "html", "css"].includes(ext))
    return <FileCode className="w-4 h-4 text-blue-400" />;
  if (["txt", "md", "log", "csv", "conf", "cfg", "ini"].includes(ext))
    return <FileText className="w-4 h-4 text-gray-400" />;
  return <File className="w-4 h-4 text-gray-500" />;
}

type SortKey = "name" | "size" | "modified";

export default function FileManager({
  deviceName,
  host,
  user,
  port,
  onClose,
}: FileManagerProps) {
  const { t } = useTranslation();
  const [currentPath, setCurrentPath] = useState(user === "root" ? "/root" : `/home/${user}`);
  const [files, setFiles] = useState<RemoteFile[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [sortBy, setSortBy] = useState<SortKey>("name");
  const [sortAsc, setSortAsc] = useState(true);
  const [showNewFolder, setShowNewFolder] = useState(false);
  const [newFolderName, setNewFolderName] = useState("");
  const [transferring, setTransferring] = useState(false);
  const [transferInfo, setTransferInfo] = useState<{
    filename: string;
    percent: number;
    direction: string;
  } | null>(null);

  // Load directory listing
  const loadDir = useCallback(
    async (path: string) => {
      setLoading(true);
      setError(null);
      setSelected(new Set());
      try {
        const result = await api.sftpList(host, user, port, path);
        setFiles(result);
        setCurrentPath(path);
      } catch (err) {
        setError(String(err));
      } finally {
        setLoading(false);
      }
    },
    [host, user, port],
  );

  useEffect(() => {
    loadDir(currentPath);
  }, []);

  // Listen for transfer progress events
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    listen<{ filename: string; percent: number; direction: string }>(
      "file-transfer-progress",
      (event) => {
        setTransferInfo(event.payload);
        if (event.payload.percent >= 100) {
          setTimeout(() => {
            setTransferInfo(null);
            setTransferring(false);
          }, 800);
        }
      },
    ).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  // Sort files
  const sortedFiles = [...files].sort((a, b) => {
    // ".." always first
    if (a.name === "..") return -1;
    if (b.name === "..") return 1;
    // Directories before files
    if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;

    let cmp = 0;
    if (sortBy === "name") {
      cmp = a.name.toLowerCase().localeCompare(b.name.toLowerCase());
    } else if (sortBy === "size") {
      cmp = a.size - b.size;
    } else if (sortBy === "modified") {
      cmp = a.modified.localeCompare(b.modified);
    }
    return sortAsc ? cmp : -cmp;
  });

  const toggleSort = (key: SortKey) => {
    if (sortBy === key) {
      setSortAsc(!sortAsc);
    } else {
      setSortBy(key);
      setSortAsc(true);
    }
  };

  const toggleSelect = (name: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(name)) {
        next.delete(name);
      } else {
        next.add(name);
      }
      return next;
    });
  };

  const navigateTo = (path: string) => {
    loadDir(path);
  };

  const navigateUp = () => {
    const parent = currentPath.replace(/\/[^/]+\/?$/, "") || "/";
    loadDir(parent);
  };

  // Breadcrumb segments
  const pathSegments = currentPath.split("/").filter(Boolean);

  // Upload handler
  const handleUpload = async () => {
    const result = await open({ multiple: true, directory: false });
    if (!result) return;

    const paths = Array.isArray(result) ? result : [result];
    setTransferring(true);

    for (const localPath of paths) {
      const filename =
        typeof localPath === "string"
          ? localPath.split("/").pop() ?? "file"
          : localPath;
      const remoteDest = `${currentPath}/${filename}`;
      try {
        await api.sftpUpload(host, user, port, String(localPath), remoteDest);
      } catch (err) {
        setError(`Upload failed: ${err}`);
        break;
      }
    }

    setTransferring(false);
    loadDir(currentPath);
  };

  // Download handler
  const handleDownload = async () => {
    const selectedFiles = files.filter(
      (f) => selected.has(f.name) && !f.is_dir && f.name !== "..",
    );
    if (selectedFiles.length === 0) return;

    for (const file of selectedFiles) {
      const localPath = await save({ defaultPath: file.name });
      if (!localPath) continue;

      setTransferring(true);
      try {
        await api.sftpDownload(host, user, port, file.path, localPath);
      } catch (err) {
        setError(`Download failed: ${err}`);
        break;
      }
    }
    setTransferring(false);
  };

  // New folder handler
  const handleNewFolder = async () => {
    if (!newFolderName.trim()) return;
    try {
      await api.sftpMkdir(host, user, port, `${currentPath}/${newFolderName.trim()}`);
      setShowNewFolder(false);
      setNewFolderName("");
      loadDir(currentPath);
    } catch (err) {
      setError(`Create folder failed: ${err}`);
    }
  };

  // Delete handler
  const handleDelete = async () => {
    const toDelete = files.filter(
      (f) => selected.has(f.name) && f.name !== "..",
    );
    if (toDelete.length === 0) return;

    const names = toDelete.map((f) => f.name).join(", ");
    if (!confirm(`Delete ${toDelete.length} item(s)?\n${names}`)) return;

    for (const file of toDelete) {
      try {
        await api.sftpDelete(host, user, port, file.path);
      } catch (err) {
        setError(`Delete failed: ${err}`);
        break;
      }
    }
    loadDir(currentPath);
  };

  const selectedCount = selected.size;
  const hasFileSelected = files.some(
    (f) => selected.has(f.name) && !f.is_dir && f.name !== "..",
  );

  return (
    <div className="flex flex-col h-full bg-surface-0">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-1.5 bg-surface-1 border-b border-surface-3 shrink-0">
        <div className="flex items-center gap-2">
          <Folder className="w-4 h-4 text-yellow-400" />
          <span className="text-sm text-gray-900 dark:text-white font-medium">{deviceName}</span>
          <span className="text-xs text-gray-400 dark:text-gray-500">{host}</span>
        </div>
        <button
          onClick={onClose}
          className="p-1 text-gray-500 dark:text-gray-400 hover:text-red-400 transition-colors"
        >
          <X className="w-4 h-4" />
        </button>
      </div>

      {/* Path bar */}
      <div className="flex items-center gap-1 px-3 py-2 bg-surface-1 border-b border-surface-3 shrink-0 overflow-x-auto">
        <button
          onClick={() => navigateTo(user === "root" ? "/root" : `/home/${user}`)}
          className="p-1 text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white shrink-0"
          title="Home"
        >
          <Home className="w-3.5 h-3.5" />
        </button>
        <button
          onClick={navigateUp}
          className="p-1 text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white shrink-0"
          title="Parent directory"
        >
          <ArrowUp className="w-3.5 h-3.5" />
        </button>
        <span className="text-gray-600 mx-1">/</span>
        {pathSegments.map((seg, i) => {
          const segPath = "/" + pathSegments.slice(0, i + 1).join("/");
          return (
            <span key={segPath} className="flex items-center shrink-0">
              <button
                onClick={() => navigateTo(segPath)}
                className="text-xs text-gray-600 dark:text-gray-300 hover:text-accent transition-colors"
              >
                {seg}
              </button>
              {i < pathSegments.length - 1 && (
                <ChevronRight className="w-3 h-3 text-gray-600 mx-0.5" />
              )}
            </span>
          );
        })}
      </div>

      {/* Action bar */}
      <div className="flex items-center gap-1.5 px-3 py-1.5 bg-surface-2 border-b border-surface-3 shrink-0">
        <button
          onClick={handleUpload}
          disabled={transferring}
          className="flex items-center gap-1 px-2.5 py-1 bg-accent/20 hover:bg-accent/30 text-accent rounded text-xs transition-colors disabled:opacity-50"
        >
          <Upload className="w-3.5 h-3.5" />
          {t('files.upload')}
        </button>
        <button
          onClick={handleDownload}
          disabled={!hasFileSelected || transferring}
          className="flex items-center gap-1 px-2.5 py-1 bg-surface-3 hover:bg-surface-1 text-gray-600 dark:text-gray-300 rounded text-xs transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
        >
          <Download className="w-3.5 h-3.5" />
          {t('files.download')}
        </button>
        <button
          onClick={() => setShowNewFolder(!showNewFolder)}
          className="flex items-center gap-1 px-2.5 py-1 bg-surface-3 hover:bg-surface-1 text-gray-600 dark:text-gray-300 rounded text-xs transition-colors"
        >
          <FolderPlus className="w-3.5 h-3.5" />
          {t('files.newFolder')}
        </button>
        <button
          onClick={handleDelete}
          disabled={selectedCount === 0}
          className="flex items-center gap-1 px-2.5 py-1 bg-surface-3 hover:bg-red-500/20 hover:text-red-400 text-gray-600 dark:text-gray-300 rounded text-xs transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
        >
          <Trash2 className="w-3.5 h-3.5" />
          {t('files.delete')}
        </button>
        <div className="flex-1" />
        <button
          onClick={() => loadDir(currentPath)}
          disabled={loading}
          className="p-1 text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white transition-colors"
          title="Refresh"
        >
          <RefreshCw className={`w-3.5 h-3.5 ${loading ? "animate-spin" : ""}`} />
        </button>
        {selectedCount > 0 && (
          <span className="text-[10px] text-gray-400 dark:text-gray-500">
            {selectedCount} selected
          </span>
        )}
      </div>

      {/* New folder input */}
      {showNewFolder && (
        <div className="flex items-center gap-2 px-3 py-2 bg-surface-2 border-b border-surface-3">
          <FolderPlus className="w-4 h-4 text-yellow-400 shrink-0" />
          <input
            autoFocus
            type="text"
            placeholder={t('files.folderName')}
            value={newFolderName}
            onChange={(e) => setNewFolderName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleNewFolder();
              if (e.key === "Escape") {
                setShowNewFolder(false);
                setNewFolderName("");
              }
            }}
            className="flex-1 px-2 py-1 bg-surface-0 border border-surface-3 rounded text-sm text-gray-900 dark:text-white placeholder-gray-400 dark:placeholder-gray-500 focus:outline-none focus:border-accent"
          />
          <button
            onClick={handleNewFolder}
            className="px-2 py-1 bg-accent hover:bg-accent-hover text-white rounded text-xs"
          >
            {t('files.create')}
          </button>
          <button
            onClick={() => {
              setShowNewFolder(false);
              setNewFolderName("");
            }}
            className="px-2 py-1 text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white text-xs"
          >
            {t('dashboard.cancel')}
          </button>
        </div>
      )}

      {/* Error banner */}
      {error && (
        <div className="flex items-center gap-2 px-3 py-2 bg-red-500/10 border-b border-red-500/20">
          <AlertCircle className="w-4 h-4 text-red-400 shrink-0" />
          <span className="text-xs text-red-400 flex-1">{error}</span>
          <button
            onClick={() => setError(null)}
            className="text-red-400 hover:text-red-300 text-xs"
          >
            Dismiss
          </button>
        </div>
      )}

      {/* File list */}
      <div className="flex-1 overflow-auto">
        {/* Column headers */}
        <div className="flex items-center px-3 py-1.5 bg-surface-1 border-b border-surface-3 text-[11px] text-gray-400 dark:text-gray-500 sticky top-0">
          <div className="w-6 shrink-0" />
          <button
            onClick={() => toggleSort("name")}
            className="flex-1 text-left hover:text-gray-900 dark:hover:text-white transition-colors"
          >
            {t('files.name')} {sortBy === "name" && (sortAsc ? "^" : "v")}
          </button>
          <button
            onClick={() => toggleSort("size")}
            className="w-20 text-right hover:text-gray-900 dark:hover:text-white transition-colors"
          >
            {t('files.size')} {sortBy === "size" && (sortAsc ? "^" : "v")}
          </button>
          <button
            onClick={() => toggleSort("modified")}
            className="w-28 text-right hover:text-gray-900 dark:hover:text-white transition-colors"
          >
            {t('files.modified')} {sortBy === "modified" && (sortAsc ? "^" : "v")}
          </button>
          <div className="w-24 text-right">{t('files.permissions')}</div>
        </div>

        {loading && files.length === 0 ? (
          <div className="flex items-center justify-center py-12">
            <Loader2 className="w-6 h-6 text-accent animate-spin" />
          </div>
        ) : (
          sortedFiles.map((file) => (
            <div
              key={file.name}
              onClick={() => {
                if (file.name !== "..") toggleSelect(file.name);
              }}
              onDoubleClick={() => {
                if (file.is_dir) {
                  if (file.name === "..") {
                    navigateUp();
                  } else {
                    navigateTo(file.path);
                  }
                }
              }}
              className={`flex items-center px-3 py-1.5 text-sm border-b border-surface-3/50 cursor-pointer transition-colors ${
                selected.has(file.name)
                  ? "bg-accent/10 text-gray-900 dark:text-white"
                  : "hover:bg-surface-2 text-gray-600 dark:text-gray-300"
              }`}
            >
              <div className="w-6 shrink-0 flex items-center justify-center">
                {fileIcon(file.name, file.is_dir)}
              </div>
              <div className="flex-1 truncate text-sm">
                {file.name}
                {file.is_dir && file.name !== ".." && "/"}
              </div>
              <div className="w-20 text-right text-xs text-gray-500">
                {file.is_dir ? "-" : formatSize(file.size)}
              </div>
              <div className="w-28 text-right text-xs text-gray-500">
                {file.modified}
              </div>
              <div className="w-24 text-right text-[10px] text-gray-600 font-mono">
                {file.permissions}
              </div>
            </div>
          ))
        )}

        {!loading && files.length === 0 && !error && (
          <div className="text-center text-gray-400 dark:text-gray-500 py-12 text-sm">
            {t('files.emptyDir')}
          </div>
        )}
      </div>

      {/* Transfer progress bar */}
      {(transferring || transferInfo) && (
        <div className="px-3 py-2 bg-surface-1 border-t border-surface-3 shrink-0">
          <div className="flex items-center gap-2 text-xs">
            <Loader2 className="w-3.5 h-3.5 text-accent animate-spin shrink-0" />
            <span className="text-gray-500 dark:text-gray-400 truncate flex-1">
              {transferInfo
                ? `${transferInfo.direction === "upload" ? "Uploading" : "Downloading"} ${transferInfo.filename}`
                : "Transferring..."}
            </span>
            <span className="text-gray-400 dark:text-gray-500">
              {transferInfo?.percent ?? 0}%
            </span>
          </div>
          <div className="mt-1 w-full bg-surface-3 rounded-full h-1 overflow-hidden">
            <div
              className="h-full bg-accent rounded-full transition-all duration-300"
              style={{ width: `${transferInfo?.percent ?? 0}%` }}
            />
          </div>
        </div>
      )}
    </div>
  );
}
