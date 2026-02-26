import { useState, useEffect } from 'react';
import { faceApi, photoApi } from '../api';
import PhotoViewer from '../components/PhotoViewer';
import { Users, ScanFace, RefreshCw, Edit3, Check, X } from 'lucide-react';

interface Person {
    id: number;
    name: string | null;
    face_count: number;
    cover_face_id: number | null;
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

export default function People() {
    const [persons, setPersons] = useState<Person[]>([]);
    const [loading, setLoading] = useState(true);
    const [scanning, setScanning] = useState(false);
    const [clustering, setClustering] = useState(false);
    const [selectedPerson, setSelectedPerson] = useState<Person | null>(null);
    const [personPhotos, setPersonPhotos] = useState<Photo[]>([]);
    const [viewerPhoto, setViewerPhoto] = useState<Photo | null>(null);
    const [editingId, setEditingId] = useState<number | null>(null);
    const [editName, setEditName] = useState('');

    const loadPersons = async () => {
        setLoading(true);
        try {
            const res = await faceApi.listPersons();
            setPersons(res.data);
        } catch (err) {
            console.error('Failed to load persons:', err);
        } finally {
            setLoading(false);
        }
    };

    useEffect(() => {
        loadPersons();
    }, []);

    const handleScan = async () => {
        setScanning(true);
        try {
            const res = await faceApi.scanFaces();
            alert(`扫描完成：处理了 ${res.data.processed} 张照片，发现 ${res.data.faces_found} 张人脸`);
            // After scan, cluster
            setClustering(true);
            const clusterRes = await faceApi.clusterFaces();
            alert(`聚类完成：创建了 ${clusterRes.data.persons_created} 个人物`);
            loadPersons();
        } catch (err) {
            console.error('Face scan failed:', err);
            alert('人脸扫描失败');
        } finally {
            setScanning(false);
            setClustering(false);
        }
    };

    const handleSelectPerson = async (person: Person) => {
        setSelectedPerson(person);
        try {
            const res = await faceApi.personPhotos(person.id);
            setPersonPhotos(res.data.data);
        } catch (err) {
            console.error('Failed to load person photos:', err);
        }
    };

    const startRename = (person: Person) => {
        setEditingId(person.id);
        setEditName(person.name || '');
    };

    const saveRename = async (personId: number) => {
        try {
            await faceApi.renamePerson(personId, editName);
            setEditingId(null);
            setPersons(prev => prev.map(p =>
                p.id === personId ? { ...p, name: editName } : p
            ));
            if (selectedPerson?.id === personId) {
                setSelectedPerson(prev => prev ? { ...prev, name: editName } : null);
            }
        } catch (err) {
            console.error('Rename failed:', err);
        }
    };

    if (loading) {
        return (
            <>
                <div className="content-header">
                    <h1>人物</h1>
                </div>
                <div className="content-body">
                    <div className="loading-spinner"><div className="spinner" /></div>
                </div>
            </>
        );
    }

    // Detail view for a selected person
    if (selectedPerson) {
        return (
            <>
                <div className="content-header">
                    <button
                        onClick={() => { setSelectedPerson(null); setPersonPhotos([]); }}
                        style={{
                            background: 'var(--bg-tertiary)', border: 'none', color: 'var(--text-primary)',
                            padding: '6px 12px', borderRadius: 'var(--radius-sm)', cursor: 'pointer',
                            marginRight: '12px', fontSize: '13px',
                        }}
                    >
                        ← 返回
                    </button>
                    <h1>{selectedPerson.name || `人物 #${selectedPerson.id}`}</h1>
                    <span style={{ color: 'var(--text-muted)', fontSize: '13px', marginLeft: '8px' }}>
                        {personPhotos.length} 张照片
                    </span>
                </div>
                <div className="content-body">
                    <div className="photo-grid">
                        {personPhotos.map(photo => (
                            <div key={photo.id} className="photo-item" onClick={() => setViewerPhoto(photo)}>
                                <img src={photoApi.thumbnailUrl(photo.id, 'small')} alt={photo.file_name} loading="lazy" />
                            </div>
                        ))}
                    </div>
                </div>
                {viewerPhoto && <PhotoViewer photo={viewerPhoto} onClose={() => setViewerPhoto(null)} />}
            </>
        );
    }

    return (
        <>
            <div className="content-header">
                <h1>人物</h1>
                <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
                    <span style={{ color: 'var(--text-muted)', fontSize: '13px' }}>
                        {persons.length} 人
                    </span>
                    <button
                        onClick={handleScan}
                        disabled={scanning || clustering}
                        className="btn-primary"
                        style={{ display: 'flex', alignItems: 'center', gap: '6px', fontSize: '13px' }}
                    >
                        {scanning ? <RefreshCw size={14} className="spin" /> : <ScanFace size={14} />}
                        {scanning ? '扫描中...' : clustering ? '聚类中...' : '扫描人脸'}
                    </button>
                </div>
            </div>

            <div className="content-body">
                {persons.length === 0 ? (
                    <div className="empty-state">
                        <Users size={48} />
                        <h2>还没有识别到人物</h2>
                        <p>点击"扫描人脸"开始检测照片中的人脸</p>
                    </div>
                ) : (
                    <div className="people-grid">
                        {persons.map((person) => (
                            <div key={person.id} className="person-card" onClick={() => handleSelectPerson(person)}>
                                <div className="person-avatar">
                                    {person.cover_face_id ? (
                                        <img src={faceApi.faceThumbnailUrl(person.cover_face_id)} alt="" />
                                    ) : (
                                        <Users size={32} />
                                    )}
                                </div>
                                <div className="person-info">
                                    {editingId === person.id ? (
                                        <div className="person-edit" onClick={e => e.stopPropagation()}>
                                            <input
                                                value={editName}
                                                onChange={e => setEditName(e.target.value)}
                                                onKeyDown={e => e.key === 'Enter' && saveRename(person.id)}
                                                autoFocus
                                            />
                                            <button onClick={() => saveRename(person.id)}><Check size={14} /></button>
                                            <button onClick={() => setEditingId(null)}><X size={14} /></button>
                                        </div>
                                    ) : (
                                        <div className="person-name-row">
                                            <span className="person-name">{person.name || `人物 #${person.id}`}</span>
                                            <button
                                                className="person-rename-btn"
                                                onClick={e => { e.stopPropagation(); startRename(person); }}
                                            >
                                                <Edit3 size={12} />
                                            </button>
                                        </div>
                                    )}
                                    <span className="person-count">{person.face_count} 张照片</span>
                                </div>
                            </div>
                        ))}
                    </div>
                )}
            </div>
        </>
    );
}
