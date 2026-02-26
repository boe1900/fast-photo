import { useState, useEffect } from 'react';
import { photoApi } from '../api';
import PhotoViewer from '../components/PhotoViewer';
import { Copy, Trash2 } from 'lucide-react';

interface Photo {
    id: number;
    file_name: string;
    taken_at: string | null;
    created_at: string;
    width: number | null;
    height: number | null;
    file_size: number;
    mime_type: string;
}

interface DuplicateGroup {
    hash: string;
    count: number;
    photos: Photo[];
}

function formatSize(bytes: number): string {
    if (bytes < 1024) return bytes + ' B';
    if (bytes < 1048576) return (bytes / 1024).toFixed(1) + ' KB';
    return (bytes / 1048576).toFixed(1) + ' MB';
}

export default function Duplicates() {
    const [groups, setGroups] = useState<DuplicateGroup[]>([]);
    const [loading, setLoading] = useState(true);
    const [viewerPhoto, setViewerPhoto] = useState<Photo | null>(null);

    const load = async () => {
        setLoading(true);
        try {
            const res = await photoApi.duplicates();
            setGroups(res.data.groups);
        } catch (err) {
            console.error('Failed to load duplicates:', err);
        } finally {
            setLoading(false);
        }
    };

    useEffect(() => { load(); }, []);

    const handleTrashDuplicate = async (id: number) => {
        await photoApi.trashPhoto(id);
        load();
    };

    return (
        <>
            <div className="content-header">
                <h1>重复照片</h1>
                <span style={{ color: 'var(--text-muted)', fontSize: '13px' }}>
                    {groups.length} 组重复
                </span>
            </div>
            <div className="content-body">
                {loading ? (
                    <div className="loading-spinner"><div className="spinner" /></div>
                ) : groups.length === 0 ? (
                    <div className="empty-state">
                        <Copy size={48} />
                        <h2>没有发现重复照片</h2>
                        <p>扫描图库后将自动检测重复文件</p>
                    </div>
                ) : (
                    <div className="duplicate-groups">
                        {groups.map((group) => (
                            <div key={group.hash} className="duplicate-group">
                                <div className="duplicate-group-header">
                                    <Copy size={16} />
                                    <span>{group.count} 张重复照片</span>
                                </div>
                                <div className="duplicate-photos">
                                    {group.photos.map((photo, idx) => (
                                        <div key={photo.id} className="duplicate-photo-item">
                                            <div className="duplicate-thumb" onClick={() => setViewerPhoto(photo)}>
                                                <img src={photoApi.thumbnailUrl(photo.id, 'small')} alt={photo.file_name} loading="lazy" />
                                                {idx === 0 && <span className="original-badge">原始</span>}
                                            </div>
                                            <div className="duplicate-info">
                                                <span className="duplicate-name">{photo.file_name}</span>
                                                <span className="duplicate-size">{formatSize(photo.file_size)}</span>
                                            </div>
                                            {idx > 0 && (
                                                <button className="duplicate-delete" onClick={() => handleTrashDuplicate(photo.id)}>
                                                    <Trash2 size={14} />
                                                </button>
                                            )}
                                        </div>
                                    ))}
                                </div>
                            </div>
                        ))}
                    </div>
                )}
            </div>
            {viewerPhoto && <PhotoViewer photo={viewerPhoto} onClose={() => setViewerPhoto(null)} />}
        </>
    );
}
