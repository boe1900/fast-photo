import { useState, useEffect } from 'react';
import { photoApi } from '../api';
import PhotoViewer from '../components/PhotoViewer';
import { FolderOpen, ArrowLeft } from 'lucide-react';

interface FolderInfo {
    name: string;
    path: string;
    photo_count: number;
}

interface Photo {
    id: number;
    file_name: string;
    taken_at: string | null;
    created_at: string;
    width: number | null;
    height: number | null;
    mime_type: string;
}

export default function Folders() {
    const [folders, setFolders] = useState<FolderInfo[]>([]);
    const [currentFolder, setCurrentFolder] = useState<string | null>(null);
    const [photos, setPhotos] = useState<Photo[]>([]);
    const [loading, setLoading] = useState(true);
    const [viewerPhoto, setViewerPhoto] = useState<Photo | null>(null);

    useEffect(() => {
        loadFolders();
    }, []);

    const loadFolders = async () => {
        setLoading(true);
        try {
            const res = await photoApi.folders();
            setFolders(res.data);
        } catch (err) {
            console.error('Failed to load folders:', err);
        } finally {
            setLoading(false);
        }
    };

    const openFolder = async (path: string) => {
        setLoading(true);
        setCurrentFolder(path);
        try {
            const res = await photoApi.folderContents(path, 1, 200);
            setPhotos(res.data.data);
        } catch (err) {
            console.error('Failed to load folder contents:', err);
        } finally {
            setLoading(false);
        }
    };

    const goBack = () => {
        setCurrentFolder(null);
        setPhotos([]);
    };

    if (loading && folders.length === 0 && photos.length === 0) {
        return (
            <>
                <div className="content-header"><h1>文件夹</h1></div>
                <div className="content-body">
                    <div className="loading-spinner"><div className="spinner" /></div>
                </div>
            </>
        );
    }

    return (
        <>
            <div className="content-header">
                <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
                    {currentFolder && (
                        <button className="btn btn-ghost btn-icon" onClick={goBack}>
                            <ArrowLeft size={18} />
                        </button>
                    )}
                    <h1>{currentFolder ? currentFolder.split('/').pop() : '文件夹'}</h1>
                </div>
            </div>

            <div className="content-body">
                {!currentFolder ? (
                    folders.length === 0 ? (
                        <div className="empty-state">
                            <FolderOpen />
                            <h2>还没有文件夹</h2>
                            <p>添加图库并扫描后，文件夹将会显示在这里</p>
                        </div>
                    ) : (
                        <div className="folder-list">
                            {folders.map((folder) => (
                                <div
                                    key={folder.path}
                                    className="folder-card"
                                    onClick={() => openFolder(folder.path)}
                                >
                                    <div className="folder-icon">
                                        <FolderOpen size={20} />
                                    </div>
                                    <div>
                                        <div className="folder-name">{folder.name}</div>
                                        <div className="folder-count">{folder.photo_count} 张照片</div>
                                    </div>
                                </div>
                            ))}
                        </div>
                    )
                ) : (
                    photos.length === 0 ? (
                        <div className="empty-state">
                            <FolderOpen />
                            <h2>空文件夹</h2>
                        </div>
                    ) : (
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
                    )
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
