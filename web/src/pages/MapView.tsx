import { useState, useEffect, useCallback } from 'react';
import { MapContainer, TileLayer, Marker, Popup, useMap } from 'react-leaflet';
import L from 'leaflet';
import { photoApi } from '../api';
import PhotoViewer from '../components/PhotoViewer';
import { MapPin } from 'lucide-react';
import axios from 'axios';
import 'leaflet/dist/leaflet.css';

// Fix leaflet default marker icon issue
const iconDefaultProto = L.Icon.Default.prototype as unknown as { _getIconUrl?: string };
delete iconDefaultProto._getIconUrl;
L.Icon.Default.mergeOptions({
    iconRetinaUrl: 'https://cdnjs.cloudflare.com/ajax/libs/leaflet/1.9.4/images/marker-icon-2x.png',
    iconUrl: 'https://cdnjs.cloudflare.com/ajax/libs/leaflet/1.9.4/images/marker-icon.png',
    shadowUrl: 'https://cdnjs.cloudflare.com/ajax/libs/leaflet/1.9.4/images/marker-shadow.png',
});

interface GeoPhoto {
    id: number;
    file_name: string;
    latitude: number;
    longitude: number;
    taken_at: string | null;
}

interface ViewerPhoto {
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

function FitBounds({ markers }: { markers: GeoPhoto[] }) {
    const map = useMap();
    useEffect(() => {
        if (markers.length > 0) {
            const bounds = L.latLngBounds(markers.map(m => [m.latitude, m.longitude]));
            map.fitBounds(bounds, { padding: [40, 40] });
        }
    }, [markers, map]);
    return null;
}

export default function MapView() {
    const [photos, setPhotos] = useState<GeoPhoto[]>([]);
    const [viewerPhoto, setViewerPhoto] = useState<ViewerPhoto | null>(null);
    const [loading, setLoading] = useState(true);

    const token = localStorage.getItem('token');
    const loadGeoPhotos = useCallback(async () => {
        try {
            const res = await axios.get('/api/photos/geo', {
                headers: { Authorization: `Bearer ${token}` },
            });
            setPhotos(res.data);
        } catch (err) {
            console.error('Failed to load geo photos:', err);
        } finally {
            setLoading(false);
        }
    }, [token]);

    useEffect(() => {
        loadGeoPhotos();
    }, [loadGeoPhotos]);

    const openPhoto = async (photo: GeoPhoto) => {
        try {
            const res = await axios.get(`/api/photos/${photo.id}`, {
                headers: { Authorization: `Bearer ${token}` },
            });
            setViewerPhoto(res.data.photo as ViewerPhoto);
        } catch (err) {
            console.error('Failed to load photo details:', err);
        }
    };

    return (
        <>
            <div className="content-header">
                <h1>
                    <MapPin size={20} style={{ display: 'inline', marginRight: '8px', verticalAlign: 'middle', color: 'var(--accent)' }} />
                    地图
                </h1>
                <span style={{ color: 'var(--text-muted)', fontSize: '13px' }}>
                    {photos.length} 张带定位的照片
                </span>
            </div>

            <div className="content-body" style={{ padding: 0, height: 'calc(100vh - 60px)' }}>
                {loading ? (
                    <div className="loading-spinner" style={{ height: '100%' }}><div className="spinner" /></div>
                ) : photos.length === 0 ? (
                    <div className="empty-state">
                        <MapPin size={48} />
                        <h3>没有带定位的照片</h3>
                        <p>拍摄照片时开启定位功能，扫描后即可在地图上查看</p>
                    </div>
                ) : (
                    <MapContainer
                        center={[35.86, 104.19]}
                        zoom={4}
                        style={{ height: '100%', width: '100%', background: 'var(--bg-primary)' }}
                    >
                        <TileLayer
                            attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>'
                            url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
                        />
                        <FitBounds markers={photos} />
                        {photos.map((photo) => (
                            <Marker
                                key={photo.id}
                                position={[photo.latitude, photo.longitude]}
                                eventHandlers={{ click: () => openPhoto(photo) }}
                            >
                                <Popup>
                                    <div style={{ textAlign: 'center', minWidth: '140px' }}>
                                        <img
                                            src={photoApi.thumbnailUrl(photo.id, 'small')}
                                            alt={photo.file_name}
                                            style={{ width: '140px', height: '100px', objectFit: 'cover', borderRadius: '4px', cursor: 'pointer' }}
                                            onClick={() => openPhoto(photo)}
                                        />
                                        <div style={{ fontSize: '11px', marginTop: '4px', color: '#666' }}>{photo.file_name}</div>
                                    </div>
                                </Popup>
                            </Marker>
                        ))}
                    </MapContainer>
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
