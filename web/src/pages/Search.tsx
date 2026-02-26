import { useState } from 'react';
import { photoApi } from '../api';
import PhotoViewer from '../components/PhotoViewer';
import { Search as SearchIcon } from 'lucide-react';

interface Photo {
    id: number;
    file_name: string;
    taken_at: string | null;
    created_at: string;
    width: number | null;
    height: number | null;
    mime_type: string;
}

export default function Search() {
    const [query, setQuery] = useState('');
    const [photos, setPhotos] = useState<Photo[]>([]);
    const [total, setTotal] = useState(0);
    const [loading, setLoading] = useState(false);
    const [searched, setSearched] = useState(false);
    const [viewerPhoto, setViewerPhoto] = useState<Photo | null>(null);

    const doSearch = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!query.trim()) return;

        setLoading(true);
        setSearched(true);
        try {
            const res = await photoApi.search(query.trim(), 1, 100);
            setPhotos(res.data.data);
            setTotal(res.data.total);
        } catch (err) {
            console.error('Search error:', err);
        } finally {
            setLoading(false);
        }
    };

    return (
        <>
            <div className="content-header">
                <h1>搜索</h1>
            </div>

            <div className="content-body">
                <form onSubmit={doSearch} style={{ marginBottom: '24px' }}>
                    <div className="search-bar" style={{ maxWidth: '500px' }}>
                        <SearchIcon size={16} />
                        <input
                            type="text"
                            placeholder="搜索照片名称、相机型号..."
                            value={query}
                            onChange={(e) => setQuery(e.target.value)}
                        />
                        <button type="submit" className="btn btn-primary" style={{ padding: '6px 14px', fontSize: '13px' }}>
                            搜索
                        </button>
                    </div>
                </form>

                {loading ? (
                    <div className="loading-spinner"><div className="spinner" /></div>
                ) : searched ? (
                    photos.length === 0 ? (
                        <div className="empty-state" style={{ height: '40vh' }}>
                            <SearchIcon />
                            <h2>未找到结果</h2>
                            <p>尝试使用不同的搜索关键词</p>
                        </div>
                    ) : (
                        <>
                            <div style={{ color: 'var(--text-muted)', fontSize: '13px', marginBottom: '16px' }}>
                                找到 {total} 个结果
                            </div>
                            <div className="photo-grid">
                                {photos.map((photo) => (
                                    <div
                                        key={photo.id}
                                        className="photo-item"
                                        onClick={() => setViewerPhoto(photo)}
                                    >
                                        <img
                                            src={photoApi.thumbnailUrl(photo.id, 'small')}
                                            alt={photo.file_name}
                                            loading="lazy"
                                        />
                                        <div className="photo-info">
                                            <span>{photo.file_name}</span>
                                        </div>
                                    </div>
                                ))}
                            </div>
                        </>
                    )
                ) : (
                    <div className="empty-state" style={{ height: '40vh' }}>
                        <SearchIcon />
                        <h2>搜索照片</h2>
                        <p>输入关键词搜索您的照片库</p>
                    </div>
                )}
            </div>

            {viewerPhoto && (
                <PhotoViewer
                    photo={viewerPhoto}
                    onClose={() => setViewerPhoto(null)}
                />
            )}
        </>
    );
}
