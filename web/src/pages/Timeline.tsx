import { useState, useEffect, useCallback, useRef } from 'react';
import type { AxiosError } from 'axios';
import { libraryApi, photoApi } from '../api';
import PhotoViewer from '../components/PhotoViewer';
import { Image, Play, Heart, Trash2, Upload, X } from 'lucide-react';
import { format } from 'date-fns';

interface Photo {
    id: number;
    file_name: string;
    taken_at: string | null;
    created_at: string;
    width: number | null;
    height: number | null;
    mime_type: string;
    is_favorite?: boolean;
    live_photo_video_path?: string | null;
    duration?: number | null;
}

const formatDuration = (seconds: number) => {
    const m = Math.floor(seconds / 60);
    const s = Math.floor(seconds % 60);
    return `${m}:${s.toString().padStart(2, '0')}`;
};

export default function Timeline() {
    const [photos, setPhotos] = useState<Photo[]>([]);
    const [total, setTotal] = useState(0);
    const [page, setPage] = useState(1);
    const [loading, setLoading] = useState(true);
    const [viewerPhoto, setViewerPhoto] = useState<Photo | null>(null);
    const [selected, setSelected] = useState<Set<number>>(new Set());
    const [uploading, setUploading] = useState(false);
    const fileInputRef = useRef<HTMLInputElement>(null);

    const loadPhotos = useCallback(async (p: number) => {
        setLoading(true);
        try {
            const res = await photoApi.timeline(p, 100);
            if (p === 1) {
                setPhotos(res.data.data);
            } else {
                setPhotos((prev) => [...prev, ...res.data.data]);
            }
            setTotal(res.data.total);
        } catch (err) {
            console.error('Failed to load photos:', err);
        } finally {
            setLoading(false);
        }
    }, []);

    useEffect(() => {
        loadPhotos(1);
    }, [loadPhotos]);

    const loadMore = () => {
        if (photos.length < total) {
            const nextPage = page + 1;
            setPage(nextPage);
            loadPhotos(nextPage);
        }
    };

    // Group photos by date
    const groupedPhotos = photos.reduce<Record<string, Photo[]>>((acc, photo) => {
        const dateStr = photo.taken_at || photo.created_at;
        const dateKey = dateStr ? format(new Date(dateStr), 'yyyy年MM月dd日') : '未知日期';
        if (!acc[dateKey]) acc[dateKey] = [];
        acc[dateKey].push(photo);
        return acc;
    }, {});

    const handleScroll = (e: React.UIEvent<HTMLDivElement>) => {
        const el = e.currentTarget;
        if (el.scrollHeight - el.scrollTop - el.clientHeight < 400) {
            loadMore();
        }
    };

    const toggleSelect = (id: number, e: React.MouseEvent) => {
        e.stopPropagation();
        setSelected(prev => {
            const next = new Set(prev);
            if (next.has(id)) next.delete(id); else next.add(id);
            return next;
        });
    };

    const handleBatchFavorite = async () => {
        await photoApi.batchFavorite([...selected], true);
        setSelected(new Set());
    };

    const handleBatchTrash = async () => {
        await photoApi.batchTrash([...selected]);
        setPhotos(prev => prev.filter(p => !selected.has(p.id)));
        setSelected(new Set());
    };

    const handleUpload = async (files: FileList) => {
        setUploading(true);
        const formData = new FormData();
        for (const file of Array.from(files)) {
            formData.append('file', file);
        }
        try {
            const libs = await libraryApi.list();
            if (!Array.isArray(libs.data) || libs.data.length === 0) {
                alert('请先在“设置 -> 图库管理”中创建图库，再上传照片。');
                return;
            }
            await photoApi.upload(formData);
            loadPhotos(1);
            setPage(1);
        } catch (err) {
            console.error('Upload failed:', err);
            const message =
                (err as AxiosError<{ error?: string }>).response?.data?.error ||
                ((err as AxiosError).response?.status === 400
                    ? '上传失败：请先创建图库。'
                    : '上传失败，请重试。');
            alert(message);
        } finally {
            setUploading(false);
        }
    };

    if (loading && photos.length === 0) {
        return (
            <>
                <div className="content-header">
                    <h1>照片</h1>
                    <span style={{ color: 'var(--text-muted)', fontSize: '13px' }}>加载中...</span>
                </div>
                <div className="content-body">
                    <div className="loading-spinner">
                        <div className="spinner" />
                    </div>
                </div>
            </>
        );
    }

    if (photos.length === 0) {
        return (
            <>
                <div className="content-header">
                    <h1>照片</h1>
                    <button className="btn-primary" onClick={() => fileInputRef.current?.click()}
                        style={{ display: 'flex', alignItems: 'center', gap: '6px', fontSize: '13px' }}>
                        <Upload size={14} /> 上传照片
                    </button>
                </div>
                <div className="content-body">
                    <div className="empty-state">
                        <Image />
                        <h2>还没有照片</h2>
                        <p>前往设置页面添加图库路径，然后扫描照片。</p>
                    </div>
                </div>
                <input ref={fileInputRef} type="file" multiple accept="image/*,video/*" hidden
                    onChange={(e) => e.target.files && handleUpload(e.target.files)} />
            </>
        );
    }

    return (
        <>
            <div className="content-header">
                <h1>照片</h1>
                <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
                    <span style={{ color: 'var(--text-muted)', fontSize: '13px' }}>
                        共 {total} 张
                    </span>
                    <button className="btn-primary" onClick={() => fileInputRef.current?.click()}
                        disabled={uploading}
                        style={{ display: 'flex', alignItems: 'center', gap: '6px', fontSize: '13px' }}>
                        <Upload size={14} /> {uploading ? '上传中...' : '上传'}
                    </button>
                </div>
            </div>

            <div className="content-body" onScroll={handleScroll}>
                {Object.entries(groupedPhotos).map(([date, datePhotos]) => (
                    <div key={date} className="timeline-group">
                        <div className="timeline-date">{date}</div>
                        <div className="photo-grid">
                            {datePhotos.map((photo) => (
                                <div
                                    key={photo.id}
                                    className={`photo-item ${selected.has(photo.id) ? 'selected' : ''}`}
                                    onClick={() => setViewerPhoto(photo)}
                                >
                                    <img
                                        src={photoApi.thumbnailUrl(photo.id, 'small')}
                                        alt={photo.file_name}
                                        loading="lazy"
                                    />
                                    <input
                                        type="checkbox"
                                        className="photo-checkbox"
                                        checked={selected.has(photo.id)}
                                        onChange={() => { }}
                                        onClick={(e) => toggleSelect(photo.id, e as unknown as React.MouseEvent)}
                                    />
                                    {photo.live_photo_video_path && (
                                        <div className="live-badge">LIVE</div>
                                    )}
                                    {photo.mime_type.startsWith('video/') && (
                                        <div className="video-badge">
                                            <Play size={10} fill="white" />
                                            {photo.duration != null && (
                                                <span>{formatDuration(photo.duration)}</span>
                                            )}
                                        </div>
                                    )}
                                    <div className="photo-info">
                                        <span>{photo.file_name}</span>
                                    </div>
                                </div>
                            ))}
                        </div>
                    </div>
                ))}

                {loading && (
                    <div className="loading-spinner" style={{ height: '80px' }}>
                        <div className="spinner" />
                    </div>
                )}
            </div>

            {/* Batch action bar */}
            {selected.size > 0 && (
                <div className="batch-bar">
                    <span>已选 {selected.size} 项</span>
                    <button onClick={handleBatchFavorite}><Heart size={14} /> 收藏</button>
                    <button onClick={handleBatchTrash}><Trash2 size={14} /> 删除</button>
                    <button onClick={() => setSelected(new Set())}><X size={14} /> 取消</button>
                </div>
            )}

            {viewerPhoto && (
                <PhotoViewer
                    photo={viewerPhoto}
                    onClose={() => setViewerPhoto(null)}
                />
            )}

            <input ref={fileInputRef} type="file" multiple accept="image/*,video/*" hidden
                onChange={(e) => e.target.files && handleUpload(e.target.files)} />
        </>
    );
}
