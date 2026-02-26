import { useState, useEffect, useRef } from 'react';
import { X, Calendar, Camera, MapPin, FileText, Disc, Tag, Plus } from 'lucide-react';
import { photoApi, tagApi } from '../api';
import { format } from 'date-fns';

interface Photo {
    id: number;
    file_name: string;
    taken_at: string | null;
    created_at: string;
    width: number | null;
    height: number | null;
    mime_type: string;
    live_photo_video_path?: string | null;
    duration?: number | null;
}

interface PhotoDetail {
    photo: {
        id: number;
        file_name: string;
        file_size: number;
        taken_at: string | null;
        width: number | null;
        height: number | null;
        camera_make: string | null;
        camera_model: string | null;
        latitude: number | null;
        longitude: number | null;
        mime_type: string;
        live_photo_video_path: string | null;
        duration: number | null;
    };
    thumbnail_url: string;
    medium_url: string;
    full_url: string;
}

interface PhotoViewerProps {
    photo: Photo;
    onClose: () => void;
}

export default function PhotoViewer({ photo, onClose }: PhotoViewerProps) {
    const [detail, setDetail] = useState<PhotoDetail | null>(null);
    const [showInfo, setShowInfo] = useState(true);
    const [isPlayingLive, setIsPlayingLive] = useState(false);
    const [tags, setTags] = useState<{ id: number; name: string; category: string }[]>([]);
    const [newTag, setNewTag] = useState('');
    const videoRef = useRef<HTMLVideoElement>(null);

    const isLivePhoto = photo.live_photo_video_path != null;
    const isVideo = photo.mime_type.startsWith('video/');

    useEffect(() => {
        photoApi.detail(photo.id).then((res) => setDetail(res.data));
        tagApi.photoTags(photo.id).then((res) => setTags(res.data.tags || []));
    }, [photo.id]);

    useEffect(() => {
        const handleKey = (e: KeyboardEvent) => {
            if (e.key === 'Escape') onClose();
            if (e.key === 'i') setShowInfo((prev) => !prev);
            // Press and hold 'l' to play Live Photo
            if (e.key === 'l' && isLivePhoto && !isPlayingLive) {
                startLivePlayback();
            }
        };
        const handleKeyUp = (e: KeyboardEvent) => {
            if (e.key === 'l' && isPlayingLive) {
                stopLivePlayback();
            }
        };
        window.addEventListener('keydown', handleKey);
        window.addEventListener('keyup', handleKeyUp);
        return () => {
            window.removeEventListener('keydown', handleKey);
            window.removeEventListener('keyup', handleKeyUp);
        };
    }, [onClose, isLivePhoto, isPlayingLive]);

    const startLivePlayback = () => {
        setIsPlayingLive(true);
        if (videoRef.current) {
            videoRef.current.currentTime = 0;
            videoRef.current.play().catch(() => { });
        }
    };

    const stopLivePlayback = () => {
        setIsPlayingLive(false);
        if (videoRef.current) {
            videoRef.current.pause();
            videoRef.current.currentTime = 0;
        }
    };

    const formatFileSize = (bytes: number) => {
        if (bytes < 1024) return `${bytes} B`;
        if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
        return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    };

    const handleAddTag = async () => {
        const name = newTag.trim();
        if (!name) return;
        try {
            await tagApi.addTag(photo.id, name);
            const res = await tagApi.photoTags(photo.id);
            setTags(res.data.tags || []);
            setNewTag('');
        } catch (err) {
            console.error('Failed to add tag:', err);
        }
    };

    const handleRemoveTag = async (tagId: number) => {
        try {
            await tagApi.removeTag(photo.id, tagId);
            setTags(prev => prev.filter(t => t.id !== tagId));
        } catch (err) {
            console.error('Failed to remove tag:', err);
        }
    };

    const p = detail?.photo;
    const liveVideoUrl = `/api/photos/${photo.id}/live-video`;

    return (
        <div className="photo-viewer-overlay" onClick={onClose}>
            <div
                className="photo-viewer-content"
                onClick={(e) => e.stopPropagation()}
                style={{ display: 'flex' }}
            >
                <div
                    style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center', position: 'relative' }}
                    onMouseDown={() => isLivePhoto && startLivePlayback()}
                    onMouseUp={() => isLivePhoto && stopLivePlayback()}
                    onMouseLeave={() => isPlayingLive && stopLivePlayback()}
                >
                    {/* Video player for video files */}
                    {isVideo ? (
                        <video
                            src={`/api/photos/${photo.id}/original`}
                            controls
                            autoPlay
                            playsInline
                            style={{
                                maxWidth: '90vw',
                                maxHeight: '90vh',
                                borderRadius: 'var(--radius-md)',
                            }}
                        />
                    ) : (
                        <>
                            {/* Static image */}
                            <img
                                src={photoApi.thumbnailUrl(photo.id, 'large')}
                                alt={photo.file_name}
                                style={{
                                    opacity: isPlayingLive ? 0 : 1,
                                    transition: 'opacity 0.2s ease',
                                }}
                            />
                        </>
                    )}

                    {/* Live Photo video overlay */}
                    {isLivePhoto && (
                        <video
                            ref={videoRef}
                            src={liveVideoUrl}
                            muted
                            playsInline
                            loop
                            style={{
                                position: 'absolute',
                                maxWidth: '90vw',
                                maxHeight: '90vh',
                                objectFit: 'contain',
                                borderRadius: 'var(--radius-md)',
                                opacity: isPlayingLive ? 1 : 0,
                                transition: 'opacity 0.2s ease',
                                pointerEvents: 'none',
                            }}
                        />
                    )}

                    {/* LIVE badge */}
                    {isLivePhoto && (
                        <div className={`live-badge-viewer ${isPlayingLive ? 'active' : ''}`}>
                            <Disc size={14} />
                            LIVE
                        </div>
                    )}

                    <button className="photo-viewer-close" onClick={onClose}>
                        <X size={18} />
                    </button>
                </div>

                {showInfo && detail && p && (
                    <div className="photo-viewer-sidebar">
                        <h2 style={{ fontSize: '15px', fontWeight: 600, marginBottom: '20px', wordBreak: 'break-word' }}>
                            {p.file_name}
                        </h2>

                        <div className="exif-section">
                            <h3><FileText size={12} style={{ display: 'inline', marginRight: '6px' }} />文件信息</h3>
                            <div className="exif-item">
                                <span className="exif-label">文件大小</span>
                                <span className="exif-value">{formatFileSize(p.file_size)}</span>
                            </div>
                            {p.width && p.height && (
                                <div className="exif-item">
                                    <span className="exif-label">尺寸</span>
                                    <span className="exif-value">{p.width} × {p.height}</span>
                                </div>
                            )}
                            <div className="exif-item">
                                <span className="exif-label">类型</span>
                                <span className="exif-value">
                                    {p.mime_type}
                                    {p.live_photo_video_path && ' (Live Photo)'}
                                </span>
                            </div>
                        </div>

                        {(p.camera_make || p.camera_model) && (
                            <div className="exif-section">
                                <h3><Camera size={12} style={{ display: 'inline', marginRight: '6px' }} />相机信息</h3>
                                {p.camera_make && (
                                    <div className="exif-item">
                                        <span className="exif-label">品牌</span>
                                        <span className="exif-value">{p.camera_make}</span>
                                    </div>
                                )}
                                {p.camera_model && (
                                    <div className="exif-item">
                                        <span className="exif-label">型号</span>
                                        <span className="exif-value">{p.camera_model}</span>
                                    </div>
                                )}
                            </div>
                        )}

                        {p.taken_at && (
                            <div className="exif-section">
                                <h3><Calendar size={12} style={{ display: 'inline', marginRight: '6px' }} />拍摄时间</h3>
                                <div className="exif-item">
                                    <span className="exif-label">日期</span>
                                    <span className="exif-value">
                                        {format(new Date(p.taken_at), 'yyyy-MM-dd HH:mm:ss')}
                                    </span>
                                </div>
                            </div>
                        )}

                        {p.latitude && p.longitude && (
                            <div className="exif-section">
                                <h3><MapPin size={12} style={{ display: 'inline', marginRight: '6px' }} />位置</h3>
                                <div className="exif-item">
                                    <span className="exif-label">纬度</span>
                                    <span className="exif-value">{p.latitude.toFixed(6)}</span>
                                </div>
                                <div className="exif-item">
                                    <span className="exif-label">经度</span>
                                    <span className="exif-value">{p.longitude.toFixed(6)}</span>
                                </div>
                            </div>
                        )}

                        {isLivePhoto && (
                            <div className="exif-section">
                                <h3><Disc size={12} style={{ display: 'inline', marginRight: '6px' }} />Live Photo</h3>
                                <div className="exif-item">
                                    <span className="exif-label" style={{ fontSize: '12px', color: 'var(--text-muted)' }}>
                                        长按图片或按住 L 键播放
                                    </span>
                                </div>
                            </div>
                        )}

                        {/* Tags section */}
                        <div className="exif-section">
                            <h3><Tag size={12} style={{ display: 'inline', marginRight: '6px' }} />标签</h3>
                            <div className="tag-chips">
                                {tags.map(tag => (
                                    <span key={tag.id} className="tag-chip">
                                        {tag.name}
                                        <button className="tag-remove" onClick={() => handleRemoveTag(tag.id)}>×</button>
                                    </span>
                                ))}
                                {tags.length === 0 && (
                                    <span style={{ fontSize: '12px', color: 'var(--text-muted)' }}>暂无标签</span>
                                )}
                            </div>
                            <div className="tag-input-row">
                                <input
                                    value={newTag}
                                    onChange={(e) => setNewTag(e.target.value)}
                                    onKeyDown={(e) => e.key === 'Enter' && (e.preventDefault(), handleAddTag())}
                                    placeholder="添加标签..."
                                />
                                <button onClick={handleAddTag}><Plus size={12} /></button>
                            </div>
                        </div>
                    </div>
                )}
            </div>
        </div>
    );
}
