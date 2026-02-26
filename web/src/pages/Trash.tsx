import { useState, useEffect } from 'react';
import { photoApi } from '../api';
import PhotoViewer from '../components/PhotoViewer';
import { Trash2, RotateCcw, AlertTriangle } from 'lucide-react';

interface Photo {
    id: number;
    file_name: string;
    taken_at: string | null;
    created_at: string;
    width: number | null;
    height: number | null;
    deleted_at: string | null;
    mime_type: string;
}

export default function Trash() {
    const [photos, setPhotos] = useState<Photo[]>([]);
    const [total, setTotal] = useState(0);
    const [loading, setLoading] = useState(true);
    const [selected, setSelected] = useState<Set<number>>(new Set());
    const [viewerPhoto, setViewerPhoto] = useState<Photo | null>(null);

    const load = async () => {
        setLoading(true);
        try {
            const res = await photoApi.trashList();
            setPhotos(res.data.data);
            setTotal(res.data.total);
        } catch (err) {
            console.error('Failed to load trash:', err);
        } finally {
            setLoading(false);
        }
    };

    useEffect(() => { load(); }, []);

    const toggleSelect = (id: number, e: React.MouseEvent) => {
        e.stopPropagation();
        setSelected(prev => {
            const next = new Set(prev);
            if (next.has(id)) next.delete(id); else next.add(id);
            return next;
        });
    };

    const handleRestore = async (ids: number[]) => {
        await photoApi.batchRestore(ids);
        setSelected(new Set());
        load();
    };

    const handleEmptyTrash = async () => {
        if (!confirm('确定要永久删除回收站中的所有照片吗？此操作不可撤销。')) return;
        await photoApi.emptyTrash();
        load();
    };

    const handlePermanentDelete = async (id: number) => {
        if (!confirm('确定永久删除？此操作不可撤销。')) return;
        await photoApi.permanentDelete(id);
        load();
    };

    return (
        <>
            <div className="content-header">
                <h1>回收站</h1>
                <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
                    <span style={{ color: 'var(--text-muted)', fontSize: '13px' }}>{total} 项</span>
                    {selected.size > 0 && (
                        <button className="btn-primary" onClick={() => handleRestore([...selected])}
                            style={{ display: 'flex', alignItems: 'center', gap: '4px', fontSize: '13px' }}>
                            <RotateCcw size={14} /> 还原 ({selected.size})
                        </button>
                    )}
                    {total > 0 && (
                        <button className="btn-danger" onClick={handleEmptyTrash}
                            style={{ display: 'flex', alignItems: 'center', gap: '4px', fontSize: '13px' }}>
                            <AlertTriangle size={14} /> 清空回收站
                        </button>
                    )}
                </div>
            </div>
            <div className="content-body">
                {loading ? (
                    <div className="loading-spinner"><div className="spinner" /></div>
                ) : photos.length === 0 ? (
                    <div className="empty-state">
                        <Trash2 size={48} />
                        <h2>回收站是空的</h2>
                        <p>删除的照片会在这里显示</p>
                    </div>
                ) : (
                    <div className="photo-grid">
                        {photos.map(photo => (
                            <div key={photo.id}
                                className={`photo-item ${selected.has(photo.id) ? 'selected' : ''}`}
                                onClick={() => setViewerPhoto(photo)}
                            >
                                <img src={photoApi.thumbnailUrl(photo.id, 'small')} alt={photo.file_name} loading="lazy" />
                                <div className="trash-actions">
                                    <button onClick={(e) => { e.stopPropagation(); handleRestore([photo.id]); }} title="还原">
                                        <RotateCcw size={14} />
                                    </button>
                                    <button onClick={(e) => { e.stopPropagation(); handlePermanentDelete(photo.id); }} title="永久删除">
                                        <Trash2 size={14} />
                                    </button>
                                </div>
                                <input
                                    type="checkbox"
                                    className="photo-checkbox"
                                    checked={selected.has(photo.id)}
                                    onChange={() => { }}
                                    onClick={(e) => toggleSelect(photo.id, e as unknown as React.MouseEvent)}
                                />
                            </div>
                        ))}
                    </div>
                )}
            </div>
            {viewerPhoto && <PhotoViewer photo={viewerPhoto} onClose={() => setViewerPhoto(null)} />}
        </>
    );
}
