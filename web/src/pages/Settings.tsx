import { useState, useEffect, useCallback } from 'react';
import { type AxiosError } from 'axios';
import { activityApi, libraryApi, storageApi, type StorageConfig } from '../api';
import { FolderPlus, Trash2, RefreshCw, HardDrive, Clock, Database, Server } from 'lucide-react';

interface Library {
    id: number;
    name: string;
    path: string;
    scan_status: string;
    photo_count: number;
    created_at: string;
}

interface ScanProgress {
    library_id: number;
    total_files: number;
    processed_files: number;
    status: string;
}

interface ActivityItem {
    id: number;
    action: string;
    detail: string | null;
    created_at: string;
}

interface S3FormState {
    endpoint: string;
    region: string;
    bucket: string;
    access_key: string;
    secret_key: string;
    prefix: string;
}

interface WebDavFormState {
    url: string;
    username: string;
    password: string;
    prefix: string;
}

const ACTION_LABELS: Record<string, string> = {
    add_tag: '添加标签',
    remove_tag: '移除标签',
    upload: '上传照片',
    trash: '删除照片',
    restore: '还原照片',
    favorite: '收藏照片',
    login: '登录',
    register: '注册',
    scan: '扫描图库',
};

function formatTime(dateStr: string): string {
    const d = new Date(dateStr);
    const now = new Date();
    const diff = now.getTime() - d.getTime();
    if (diff < 60000) return '刚刚';
    if (diff < 3600000) return `${Math.floor(diff / 60000)} 分钟前`;
    if (diff < 86400000) return `${Math.floor(diff / 3600000)} 小时前`;
    return d.toLocaleDateString('zh-CN');
}

export default function Settings() {
    const [libraries, setLibraries] = useState<Library[]>([]);
    const [loading, setLoading] = useState(true);
    const [showAddModal, setShowAddModal] = useState(false);
    const [newName, setNewName] = useState('');
    const [newPath, setNewPath] = useState('');
    const [addError, setAddError] = useState('');
    const [scanProgress, setScanProgress] = useState<ScanProgress | null>(null);
    const [activities, setActivities] = useState<ActivityItem[]>([]);
    const [activeTab, setActiveTab] = useState<'libraries' | 'storage' | 'activity'>('libraries');
    const [activeStorageType, setActiveStorageType] = useState<'local' | 's3' | 'webdav'>('local');
    const [localPath, setLocalPath] = useState('');
    const [s3Form, setS3Form] = useState<S3FormState>({
        endpoint: '',
        region: '',
        bucket: '',
        access_key: '',
        secret_key: '',
        prefix: '',
    });
    const [webDavForm, setWebDavForm] = useState<WebDavFormState>({
        url: '',
        username: '',
        password: '',
        prefix: '',
    });
    const [storageLoading, setStorageLoading] = useState(true);
    const [storageSaving, setStorageSaving] = useState<'local' | 's3' | 'webdav' | null>(null);
    const [storageTesting, setStorageTesting] = useState<'local' | 's3' | 'webdav' | null>(null);
    const [storageMessage, setStorageMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

    const loadLibraries = useCallback(async () => {
        try {
            const res = await libraryApi.list();
            setLibraries(res.data);
        } catch (err) {
            console.error('Failed to load libraries:', err);
        } finally {
            setLoading(false);
        }
    }, []);

    const loadActivity = useCallback(async () => {
        try {
            const res = await activityApi.list();
            setActivities(res.data.logs || []);
        } catch (err) {
            console.error('Failed to load activity:', err);
        }
    }, []);

    const applyStorageConfig = useCallback((cfg: StorageConfig) => {
        setActiveStorageType(cfg.type);
        if (cfg.type === 'local') {
            setLocalPath(cfg.path);
            return;
        }
        if (cfg.type === 's3') {
            setS3Form({
                endpoint: cfg.endpoint ?? '',
                region: cfg.region,
                bucket: cfg.bucket,
                access_key: cfg.access_key,
                secret_key: cfg.secret_key,
                prefix: cfg.prefix ?? '',
            });
            return;
        }
        setWebDavForm({
            url: cfg.url,
            username: cfg.username,
            password: cfg.password,
            prefix: cfg.prefix ?? '',
        });
    }, []);

    const loadStorageConfig = useCallback(async () => {
        try {
            const res = await storageApi.get();
            applyStorageConfig(res.data);
        } catch (err) {
            console.error('Failed to load storage config:', err);
            setStorageMessage({ type: 'error', text: '存储配置加载失败' });
        } finally {
            setStorageLoading(false);
        }
    }, [applyStorageConfig]);

    const buildStorageConfig = useCallback((target: 'local' | 's3' | 'webdav'): StorageConfig => {
        if (target === 'local') {
            return {
                type: 'local',
                path: localPath.trim(),
            };
        }
        if (target === 's3') {
            return {
                type: 's3',
                bucket: s3Form.bucket.trim(),
                region: s3Form.region.trim(),
                endpoint: s3Form.endpoint.trim() || null,
                access_key: s3Form.access_key.trim(),
                secret_key: s3Form.secret_key.trim(),
                prefix: s3Form.prefix.trim() || null,
            };
        }
        return {
            type: 'webdav',
            url: webDavForm.url.trim(),
            username: webDavForm.username.trim(),
            password: webDavForm.password.trim(),
            prefix: webDavForm.prefix.trim() || null,
        };
    }, [localPath, s3Form, webDavForm]);

    const handleTestStorage = useCallback(async (target: 'local' | 's3' | 'webdav') => {
        setStorageTesting(target);
        setStorageMessage(null);
        try {
            const res = await storageApi.test(buildStorageConfig(target));
            setStorageMessage({ type: 'success', text: res.data.message || '连接成功' });
        } catch (err) {
            const msg = (err as AxiosError<{ error?: string }>).response?.data?.error || '连接测试失败';
            setStorageMessage({ type: 'error', text: msg });
        } finally {
            setStorageTesting(null);
        }
    }, [buildStorageConfig]);

    const handleSaveStorage = useCallback(async (target: 'local' | 's3' | 'webdav') => {
        setStorageSaving(target);
        setStorageMessage(null);
        try {
            const res = await storageApi.save(buildStorageConfig(target));
            applyStorageConfig(res.data);
            setStorageMessage({ type: 'success', text: '存储配置已保存' });
        } catch (err) {
            const msg = (err as AxiosError<{ error?: string }>).response?.data?.error || '保存失败';
            setStorageMessage({ type: 'error', text: msg });
        } finally {
            setStorageSaving(null);
        }
    }, [applyStorageConfig, buildStorageConfig]);

    useEffect(() => {
        loadLibraries();
        loadActivity();
        loadStorageConfig();
    }, [loadLibraries, loadActivity, loadStorageConfig]);

    // Poll scan progress
    useEffect(() => {
        const interval = setInterval(async () => {
            try {
                const res = await libraryApi.scanProgress();
                const progress: ScanProgress = res.data;
                if (progress.status === 'scanning') {
                    setScanProgress(progress);
                } else {
                    setScanProgress(null);
                    loadLibraries();
                }
            } catch { /* ignore */ }
        }, 2000);
        return () => clearInterval(interval);
    }, [loadLibraries]);

    const handleAdd = async (e: React.FormEvent) => {
        e.preventDefault();
        setAddError('');
        try {
            await libraryApi.create(newName, newPath);
            setShowAddModal(false);
            setNewName('');
            setNewPath('');
            loadLibraries();
        } catch (err) {
            setAddError((err as AxiosError<{ error?: string }>).response?.data?.error || '创建失败');
        }
    };

    const handleDelete = async (id: number) => {
        if (!window.confirm('确定要删除此图库吗？（不会删除原始照片文件）')) return;
        try {
            await libraryApi.delete(id);
            loadLibraries();
        } catch (err) {
            console.error('Delete error:', err);
        }
    };

    const handleScan = async (id: number) => {
        try {
            await libraryApi.scan(id);
            setScanProgress({ library_id: id, total_files: 0, processed_files: 0, status: 'scanning' });
        } catch (err) {
            console.error('Scan error:', err);
        }
    };

    const tabs = [
        { key: 'libraries' as const, label: '图库管理', icon: <HardDrive size={16} /> },
        { key: 'storage' as const, label: '存储配置', icon: <Server size={16} /> },
        { key: 'activity' as const, label: '活动日志', icon: <Clock size={16} /> },
    ];
    const isLocalStorage = activeStorageType === 'local';
    const libraryPathLabel = isLocalStorage ? '本地文件夹路径' : '远端前缀路径';
    const libraryPathPlaceholder = isLocalStorage ? '例如：/Users/cola/Pictures' : '例如：team/photos';
    const libraryPathHint = isLocalStorage
        ? '本地存储：填写本机已存在的绝对路径（如 /Users/cola/Pictures）。'
        : 'S3/WebDAV：填写远端存储中的前缀路径（不是本机路径），如 team/photos。';

    return (
        <>
            <div className="content-header">
                <h1>设置</h1>
            </div>

            <div className="content-body">
                {/* Tab bar */}
                <div style={{ display: 'flex', gap: '4px', marginBottom: '20px', borderBottom: '1px solid var(--border-color)', paddingBottom: '8px' }}>
                    {tabs.map(tab => (
                        <button
                            key={tab.key}
                            onClick={() => setActiveTab(tab.key)}
                            style={{
                                display: 'flex', alignItems: 'center', gap: '6px',
                                padding: '8px 16px', borderRadius: 'var(--radius-sm)',
                                border: 'none', cursor: 'pointer', fontSize: '13px', fontWeight: 500,
                                background: activeTab === tab.key ? 'var(--accent-subtle)' : 'transparent',
                                color: activeTab === tab.key ? 'var(--accent)' : 'var(--text-secondary)',
                            }}
                        >
                            {tab.icon} {tab.label}
                        </button>
                    ))}
                </div>

                {/* Libraries tab */}
                {activeTab === 'libraries' && (
                    <div className="settings-section">
                        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: '16px' }}>
                            <h2 style={{ margin: 0 }}>图库管理</h2>
                            <button className="btn btn-primary" onClick={() => setShowAddModal(true)}>
                                <FolderPlus size={16} />
                                添加图库
                            </button>
                        </div>

                        {loading ? (
                            <div className="loading-spinner"><div className="spinner" /></div>
                        ) : libraries.length === 0 ? (
                            <div className="empty-state" style={{ height: '30vh' }}>
                                <HardDrive />
                                <h2>还没有图库</h2>
                                <p>添加图库路径来导入您的照片</p>
                                <button className="btn btn-secondary" onClick={() => setShowAddModal(true)}>
                                    <FolderPlus size={16} />
                                    添加图库
                                </button>
                            </div>
                        ) : (
                            <div className="library-list">
                                {libraries.map((lib) => (
                                    <div key={lib.id} className="library-card">
                                        <div className="library-info">
                                            <h3>{lib.name}</h3>
                                            <p>
                                                {lib.path} · {lib.photo_count} 张照片
                                            </p>
                                            {scanProgress?.library_id === lib.id && scanProgress.status === 'scanning' && (
                                                <div className="scan-progress">
                                                    <div className="progress-bar">
                                                        <div
                                                            className="progress-fill"
                                                            style={{
                                                                width: scanProgress.total_files > 0
                                                                    ? `${(scanProgress.processed_files / scanProgress.total_files) * 100}%`
                                                                    : '0%',
                                                            }}
                                                        />
                                                    </div>
                                                    <span className="progress-text">
                                                        {scanProgress.processed_files}/{scanProgress.total_files}
                                                    </span>
                                                </div>
                                            )}
                                        </div>
                                        <div className="library-actions">
                                            <span className={`badge ${lib.scan_status === 'completed' ? 'badge-success' : lib.scan_status === 'scanning' ? 'badge-warning' : 'badge-info'}`}>
                                                {lib.scan_status === 'completed' ? '已完成' : lib.scan_status === 'scanning' ? '扫描中' : '未扫描'}
                                            </span>
                                            <button
                                                className="btn btn-secondary btn-icon"
                                                onClick={() => handleScan(lib.id)}
                                                title="扫描"
                                                disabled={scanProgress?.library_id === lib.id && scanProgress?.status === 'scanning'}
                                            >
                                                <RefreshCw size={14} />
                                            </button>
                                            <button
                                                className="btn btn-secondary btn-icon"
                                                onClick={() => handleDelete(lib.id)}
                                                title="删除"
                                                style={{ color: 'var(--danger)' }}
                                            >
                                                <Trash2 size={14} />
                                            </button>
                                        </div>
                                    </div>
                                ))}
                            </div>
                        )}
                    </div>
                )}

                {/* Storage config tab */}
                {activeTab === 'storage' && (
                    <div className="settings-section">
                        <h2 style={{ marginBottom: '16px' }}>存储配置</h2>
                        <p style={{ color: 'var(--text-muted)', fontSize: '13px', marginBottom: '20px' }}>
                            配置外部存储后端。默认使用本地文件系统。
                        </p>
                        {storageMessage && (
                            <div
                                style={{
                                    marginBottom: '12px',
                                    padding: '10px 12px',
                                    borderRadius: 'var(--radius-sm)',
                                    fontSize: '13px',
                                    border: '1px solid var(--border-color)',
                                    color: storageMessage.type === 'success' ? 'var(--success)' : 'var(--danger)',
                                }}
                            >
                                {storageMessage.text}
                            </div>
                        )}

                        {storageLoading ? (
                            <div className="loading-spinner"><div className="spinner" /></div>
                        ) : (
                            <div className="storage-options">
                                {/* Local storage */}
                                <div
                                    className={`storage-card ${activeStorageType === 'local' ? 'active' : ''}`}
                                    style={{ flexDirection: 'column', alignItems: 'stretch' }}
                                >
                                    <div style={{ display: 'flex', alignItems: 'center', gap: '14px' }}>
                                        <Database size={20} style={{ flexShrink: 0 }} />
                                        <div style={{ flex: 1 }}>
                                            <h3>本地存储</h3>
                                            <p>使用服务器本地文件系统存储照片</p>
                                        </div>
                                        <span className={`badge ${activeStorageType === 'local' ? 'badge-success' : 'badge-info'}`}>
                                            {activeStorageType === 'local' ? '当前使用' : '未启用'}
                                        </span>
                                    </div>
                                    <div style={{ marginTop: '14px', display: 'flex', flexDirection: 'column', gap: '8px' }}>
                                        <div className="form-group" style={{ margin: 0 }}>
                                            <label style={{ fontSize: '12px' }}>本地存储路径</label>
                                            <input
                                                className="form-input"
                                                placeholder="/data/photos"
                                                style={{ fontSize: '12px' }}
                                                aria-label="本地存储路径"
                                                value={localPath}
                                                onChange={(e) => setLocalPath(e.target.value)}
                                            />
                                        </div>
                                        <div style={{ display: 'flex', gap: '8px', justifyContent: 'flex-end' }}>
                                            <button
                                                className="btn btn-secondary"
                                                style={{ fontSize: '12px', padding: '5px 12px' }}
                                                onClick={() => handleTestStorage('local')}
                                                disabled={storageTesting !== null || storageSaving !== null}
                                            >
                                                {storageTesting === 'local' ? '测试中...' : '测试本地连接'}
                                            </button>
                                            <button
                                                className="btn btn-primary"
                                                style={{ fontSize: '12px', padding: '5px 12px' }}
                                                onClick={() => handleSaveStorage('local')}
                                                disabled={storageTesting !== null || storageSaving !== null}
                                            >
                                                {storageSaving === 'local' ? '保存中...' : '保存本地配置'}
                                            </button>
                                        </div>
                                    </div>
                                </div>

                                {/* S3 config */}
                                <div className="storage-card" style={{ flexDirection: 'column', alignItems: 'stretch' }}>
                                    <div style={{ display: 'flex', alignItems: 'center', gap: '14px' }}>
                                        <Server size={20} style={{ flexShrink: 0 }} />
                                        <div style={{ flex: 1 }}>
                                            <h3>S3 兼容存储</h3>
                                            <p>Amazon S3、MinIO、Cloudflare R2 等</p>
                                        </div>
                                        <span className={`badge ${activeStorageType === 's3' ? 'badge-success' : 'badge-info'}`}>
                                            {activeStorageType === 's3' ? '当前使用' : '未启用'}
                                        </span>
                                    </div>
                                    <div style={{ marginTop: '14px', display: 'flex', flexDirection: 'column', gap: '8px' }}>
                                        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '8px' }}>
                                            <div className="form-group" style={{ margin: 0 }}>
                                                <label style={{ fontSize: '12px' }}>Endpoint</label>
                                                <input
                                                    className="form-input"
                                                    placeholder="https://s3.amazonaws.com"
                                                    style={{ fontSize: '12px' }}
                                                    value={s3Form.endpoint}
                                                    onChange={(e) => setS3Form((prev) => ({ ...prev, endpoint: e.target.value }))}
                                                />
                                            </div>
                                            <div className="form-group" style={{ margin: 0 }}>
                                                <label style={{ fontSize: '12px' }}>Region</label>
                                                <input
                                                    className="form-input"
                                                    placeholder="us-east-1"
                                                    style={{ fontSize: '12px' }}
                                                    value={s3Form.region}
                                                    onChange={(e) => setS3Form((prev) => ({ ...prev, region: e.target.value }))}
                                                />
                                            </div>
                                        </div>
                                        <div className="form-group" style={{ margin: 0 }}>
                                            <label style={{ fontSize: '12px' }}>Bucket</label>
                                            <input
                                                className="form-input"
                                                placeholder="my-photos-bucket"
                                                style={{ fontSize: '12px' }}
                                                value={s3Form.bucket}
                                                onChange={(e) => setS3Form((prev) => ({ ...prev, bucket: e.target.value }))}
                                            />
                                        </div>
                                        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '8px' }}>
                                            <div className="form-group" style={{ margin: 0 }}>
                                                <label style={{ fontSize: '12px' }}>Access Key</label>
                                                <input
                                                    className="form-input"
                                                    placeholder="AKIAIOSFODNN7"
                                                    style={{ fontSize: '12px' }}
                                                    value={s3Form.access_key}
                                                    onChange={(e) => setS3Form((prev) => ({ ...prev, access_key: e.target.value }))}
                                                />
                                            </div>
                                            <div className="form-group" style={{ margin: 0 }}>
                                                <label style={{ fontSize: '12px' }}>Secret Key</label>
                                                <input
                                                    className="form-input"
                                                    type="password"
                                                    placeholder="••••••••"
                                                    style={{ fontSize: '12px' }}
                                                    value={s3Form.secret_key}
                                                    onChange={(e) => setS3Form((prev) => ({ ...prev, secret_key: e.target.value }))}
                                                />
                                            </div>
                                        </div>
                                        <div className="form-group" style={{ margin: 0 }}>
                                            <label style={{ fontSize: '12px' }}>前缀路径（可选）</label>
                                            <input
                                                className="form-input"
                                                placeholder="photos"
                                                style={{ fontSize: '12px' }}
                                                value={s3Form.prefix}
                                                onChange={(e) => setS3Form((prev) => ({ ...prev, prefix: e.target.value }))}
                                            />
                                        </div>
                                        <div style={{ display: 'flex', gap: '8px', justifyContent: 'flex-end' }}>
                                            <button
                                                className="btn btn-secondary"
                                                style={{ fontSize: '12px', padding: '5px 12px' }}
                                                onClick={() => handleTestStorage('s3')}
                                                disabled={storageTesting !== null || storageSaving !== null}
                                            >
                                                {storageTesting === 's3' ? '测试中...' : '测试 S3 连接'}
                                            </button>
                                            <button
                                                className="btn btn-primary"
                                                style={{ fontSize: '12px', padding: '5px 12px' }}
                                                onClick={() => handleSaveStorage('s3')}
                                                disabled={storageTesting !== null || storageSaving !== null}
                                            >
                                                {storageSaving === 's3' ? '保存中...' : '保存 S3 配置'}
                                            </button>
                                        </div>
                                    </div>
                                </div>

                                {/* WebDAV config */}
                                <div className="storage-card" style={{ flexDirection: 'column', alignItems: 'stretch' }}>
                                    <div style={{ display: 'flex', alignItems: 'center', gap: '14px' }}>
                                        <HardDrive size={20} style={{ flexShrink: 0 }} />
                                        <div style={{ flex: 1 }}>
                                            <h3>WebDAV</h3>
                                            <p>通过 WebDAV 协议连接远程存储</p>
                                        </div>
                                        <span className={`badge ${activeStorageType === 'webdav' ? 'badge-success' : 'badge-info'}`}>
                                            {activeStorageType === 'webdav' ? '当前使用' : '未启用'}
                                        </span>
                                    </div>
                                    <div style={{ marginTop: '14px', display: 'flex', flexDirection: 'column', gap: '8px' }}>
                                        <div className="form-group" style={{ margin: 0 }}>
                                            <label style={{ fontSize: '12px' }}>WebDAV URL</label>
                                            <input
                                                className="form-input"
                                                placeholder="https://example.com/webdav"
                                                style={{ fontSize: '12px' }}
                                                value={webDavForm.url}
                                                onChange={(e) => setWebDavForm((prev) => ({ ...prev, url: e.target.value }))}
                                            />
                                        </div>
                                        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '8px' }}>
                                            <div className="form-group" style={{ margin: 0 }}>
                                                <label style={{ fontSize: '12px' }}>用户名</label>
                                                <input
                                                    className="form-input"
                                                    placeholder="admin"
                                                    style={{ fontSize: '12px' }}
                                                    value={webDavForm.username}
                                                    onChange={(e) => setWebDavForm((prev) => ({ ...prev, username: e.target.value }))}
                                                />
                                            </div>
                                            <div className="form-group" style={{ margin: 0 }}>
                                                <label style={{ fontSize: '12px' }}>密码</label>
                                                <input
                                                    className="form-input"
                                                    type="password"
                                                    placeholder="••••••••"
                                                    style={{ fontSize: '12px' }}
                                                    value={webDavForm.password}
                                                    onChange={(e) => setWebDavForm((prev) => ({ ...prev, password: e.target.value }))}
                                                />
                                            </div>
                                        </div>
                                        <div className="form-group" style={{ margin: 0 }}>
                                            <label style={{ fontSize: '12px' }}>基础路径（可选）</label>
                                            <input
                                                className="form-input"
                                                placeholder="/photos"
                                                style={{ fontSize: '12px' }}
                                                value={webDavForm.prefix}
                                                onChange={(e) => setWebDavForm((prev) => ({ ...prev, prefix: e.target.value }))}
                                            />
                                        </div>
                                        <div style={{ display: 'flex', gap: '8px', justifyContent: 'flex-end' }}>
                                            <button
                                                className="btn btn-secondary"
                                                style={{ fontSize: '12px', padding: '5px 12px' }}
                                                onClick={() => handleTestStorage('webdav')}
                                                disabled={storageTesting !== null || storageSaving !== null}
                                            >
                                                {storageTesting === 'webdav' ? '测试中...' : '测试 WebDAV 连接'}
                                            </button>
                                            <button
                                                className="btn btn-primary"
                                                style={{ fontSize: '12px', padding: '5px 12px' }}
                                                onClick={() => handleSaveStorage('webdav')}
                                                disabled={storageTesting !== null || storageSaving !== null}
                                            >
                                                {storageSaving === 'webdav' ? '保存中...' : '保存 WebDAV 配置'}
                                            </button>
                                        </div>
                                    </div>
                                </div>
                            </div>
                        )}
                    </div>
                )}

                {/* Activity log tab */}
                {activeTab === 'activity' && (
                    <div className="settings-section">
                        <h2 style={{ marginBottom: '16px' }}>活动日志</h2>
                        {activities.length === 0 ? (
                            <div className="empty-state" style={{ height: '20vh' }}>
                                <Clock size={32} />
                                <p>暂无操作记录</p>
                            </div>
                        ) : (
                            <div className="activity-list">
                                {activities.map(act => (
                                    <div key={act.id} className="activity-item">
                                        <div className="activity-icon">
                                            <Clock size={16} />
                                        </div>
                                        <div className="activity-text">
                                            {ACTION_LABELS[act.action] || act.action}
                                            {act.detail && <span style={{ color: 'var(--text-muted)', marginLeft: '6px' }}>{act.detail}</span>}
                                        </div>
                                        <div className="activity-time">{formatTime(act.created_at)}</div>
                                    </div>
                                ))}
                            </div>
                        )}
                    </div>
                )}
            </div>

            {/* Add library modal */}
            {showAddModal && (
                <div className="modal-overlay" onClick={() => setShowAddModal(false)}>
                    <div className="modal-content" onClick={(e) => e.stopPropagation()}>
                        <h2>添加图库</h2>
                        {addError && <div className="login-error">{addError}</div>}
                        <form onSubmit={handleAdd}>
                            <div className="form-group">
                                <label>图库名称</label>
                                <input
                                    className="form-input"
                                    type="text"
                                    placeholder="例如：家庭照片"
                                    value={newName}
                                    onChange={(e) => setNewName(e.target.value)}
                                    required
                                />
                            </div>
                            <div className="form-group">
                                <label>{libraryPathLabel}</label>
                                <input
                                    className="form-input"
                                    type="text"
                                    placeholder={libraryPathPlaceholder}
                                    value={newPath}
                                    onChange={(e) => setNewPath(e.target.value)}
                                    required
                                />
                                <p style={{ marginTop: '6px', fontSize: '12px', color: 'var(--text-muted)' }}>
                                    {libraryPathHint}
                                </p>
                            </div>
                            <div className="modal-actions">
                                <button type="button" className="btn btn-secondary" onClick={() => setShowAddModal(false)}>
                                    取消
                                </button>
                                <button type="submit" className="btn btn-primary">
                                    添加
                                </button>
                            </div>
                        </form>
                    </div>
                </div>
            )}
        </>
    );
}
