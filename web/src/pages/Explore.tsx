import { useState, useEffect } from 'react';
import { photoApi } from '../api';
import PhotoViewer from '../components/PhotoViewer';
import { Sparkles, Search as SearchIcon, Tag, Loader } from 'lucide-react';
import axios from 'axios';

interface Photo {
    id: number;
    file_name: string;
    taken_at: string | null;
    created_at: string;
    width: number | null;
    height: number | null;
    mime_type: string;
}

interface TagInfo {
    id: number;
    name: string;
    name_zh: string;
    category: string;
    photo_count: number;
}

interface SemanticResult {
    photo: Photo;
    score: number;
}

export default function Explore() {
    const [tags, setTags] = useState<TagInfo[]>([]);
    const [selectedTag, setSelectedTag] = useState<TagInfo | null>(null);
    const [tagPhotos, setTagPhotos] = useState<Photo[]>([]);
    const [semanticQuery, setSemanticQuery] = useState('');
    const [semanticResults, setSemanticResults] = useState<SemanticResult[]>([]);
    const [searching, setSearching] = useState(false);
    const [processing, setProcessing] = useState(false);
    const [viewerPhoto, setViewerPhoto] = useState<Photo | null>(null);
    const [loading, setLoading] = useState(true);

    const token = localStorage.getItem('token');
    const headers = { Authorization: `Bearer ${token}` };

    useEffect(() => {
        loadTags();
    }, []);

    const loadTags = async () => {
        try {
            const res = await axios.get('/api/ai/tags', { headers });
            setTags(res.data);
        } catch (err) {
            console.error('Failed to load tags:', err);
        } finally {
            setLoading(false);
        }
    };

    const selectTag = async (tag: TagInfo) => {
        setSelectedTag(tag);
        try {
            const res = await axios.get(`/api/ai/tags/${tag.id}/photos`, {
                headers,
                params: { per_page: 100 },
            });
            setTagPhotos(res.data.data);
        } catch (err) {
            console.error('Failed to load tag photos:', err);
        }
    };

    const handleSemanticSearch = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!semanticQuery.trim()) return;

        setSearching(true);
        setSelectedTag(null);
        try {
            const res = await axios.get('/api/ai/semantic-search', {
                headers,
                params: { q: semanticQuery, limit: 50 },
            });
            setSemanticResults(res.data.data);
        } catch (err: any) {
            const msg = err.response?.data?.error || '搜索失败';
            alert(msg);
        } finally {
            setSearching(false);
        }
    };

    const triggerAiProcessing = async () => {
        setProcessing(true);
        try {
            await axios.post('/api/ai/process', null, { headers });
            alert('AI 处理已在后台开始，处理完成后刷新页面查看结果');
        } catch (err: any) {
            alert(err.response?.data?.error || 'AI 处理失败');
        } finally {
            setProcessing(false);
        }
    };

    return (
        <>
            <div className="content-header">
                <h1>
                    <Sparkles size={20} style={{ display: 'inline', marginRight: '8px', verticalAlign: 'middle', color: 'var(--accent)' }} />
                    智能发现
                </h1>
                <div className="header-actions">
                    <button
                        className="btn btn-primary"
                        onClick={triggerAiProcessing}
                        disabled={processing}
                    >
                        {processing ? <Loader size={14} className="spinner" /> : <Sparkles size={14} />}
                        {processing ? 'AI 处理中...' : 'AI 分析照片'}
                    </button>
                </div>
            </div>

            <div className="content-body">
                {/* Semantic search */}
                <div style={{ marginBottom: '28px' }}>
                    <h2 style={{ fontSize: '15px', fontWeight: 600, marginBottom: '12px', color: 'var(--text-secondary)' }}>
                        🔍 语义搜索
                    </h2>
                    <form onSubmit={handleSemanticSearch} style={{ marginBottom: '16px' }}>
                        <div className="search-bar" style={{ maxWidth: '500px' }}>
                            <SearchIcon size={16} />
                            <input
                                type="text"
                                placeholder="用自然语言描述想找的照片，支持中英文..."
                                value={semanticQuery}
                                onChange={(e) => setSemanticQuery(e.target.value)}
                            />
                            <button type="submit" className="btn btn-primary" style={{ padding: '6px 14px', fontSize: '13px' }} disabled={searching}>
                                {searching ? '搜索中...' : '搜索'}
                            </button>
                        </div>
                        <p style={{ fontSize: '12px', color: 'var(--text-muted)', marginTop: '8px' }}>
                            示例: "sunset over the ocean", "a cat on a couch", "city skyline at night"
                        </p>
                    </form>

                    {semanticResults.length > 0 && (
                        <>
                            <div style={{ color: 'var(--text-muted)', fontSize: '13px', marginBottom: '12px' }}>
                                找到 {semanticResults.length} 个结果 (语义相似度排序)
                            </div>
                            <div className="photo-grid">
                                {semanticResults.map((result) => (
                                    <div
                                        key={result.photo.id}
                                        className="photo-item"
                                        onClick={() => setViewerPhoto(result.photo)}
                                    >
                                        <img
                                            src={photoApi.thumbnailUrl(result.photo.id, 'small')}
                                            alt={result.photo.file_name}
                                            loading="lazy"
                                        />
                                        <div className="photo-info">
                                            <span>{(result.score * 100).toFixed(1)}%</span>
                                        </div>
                                    </div>
                                ))}
                            </div>
                        </>
                    )}
                </div>

                {/* Scene tags */}
                <div>
                    <h2 style={{ fontSize: '15px', fontWeight: 600, marginBottom: '12px', color: 'var(--text-secondary)' }}>
                        🏷️ 场景分类
                    </h2>
                    {loading ? (
                        <div className="loading-spinner" style={{ height: '120px' }}><div className="spinner" /></div>
                    ) : tags.length === 0 ? (
                        <div style={{ color: 'var(--text-muted)', fontSize: '13.5px', padding: '20px 0' }}>
                            还没有场景标签。请先点击右上角"AI 分析照片"来处理图库中的照片。
                        </div>
                    ) : (
                        <>
                            <div style={{ display: 'flex', flexWrap: 'wrap', gap: '8px', marginBottom: '20px' }}>
                                {tags.map((tag) => (
                                    <button
                                        key={tag.id}
                                        className={`btn ${selectedTag?.id === tag.id ? 'btn-primary' : 'btn-secondary'}`}
                                        onClick={() => selectTag(tag)}
                                        style={{ fontSize: '13px' }}
                                    >
                                        <Tag size={12} />
                                        {tag.name_zh} ({tag.photo_count})
                                    </button>
                                ))}
                            </div>

                            {selectedTag && tagPhotos.length > 0 && (
                                <>
                                    <div style={{ color: 'var(--text-muted)', fontSize: '13px', marginBottom: '12px' }}>
                                        {selectedTag.name_zh} · {tagPhotos.length} 张照片
                                    </div>
                                    <div className="photo-grid">
                                        {tagPhotos.map((photo) => (
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
                            )}
                        </>
                    )}
                </div>
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
