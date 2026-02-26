import { useNavigate, useLocation } from 'react-router-dom';
import { useState, useEffect } from 'react';
import { Camera, FolderOpen, Search, Settings, LogOut, Image, Sparkles, MapPin, BookImage, Users, Heart, Trash2, Copy, Sun, Moon } from 'lucide-react';
import type { ReactNode } from 'react';

interface LayoutProps {
    user: { id: number; username: string; role: string };
    onLogout: () => void;
    children: ReactNode;
}

export default function Layout({ user, onLogout, children }: LayoutProps) {
    const navigate = useNavigate();
    const location = useLocation();
    const [theme, setTheme] = useState(() => localStorage.getItem('theme') || 'dark');

    useEffect(() => {
        document.documentElement.setAttribute('data-theme', theme);
        localStorage.setItem('theme', theme);
    }, [theme]);

    const toggleTheme = () => setTheme(prev => prev === 'dark' ? 'light' : 'dark');

    const navItems = [
        { path: '/', label: '照片', icon: <Image /> },
        { path: '/folders', label: '文件夹', icon: <FolderOpen /> },
        { path: '/explore', label: '发现', icon: <Sparkles /> },
        { path: '/favorites', label: '收藏', icon: <Heart /> },
        { path: '/people', label: '人物', icon: <Users /> },
        { path: '/map', label: '地图', icon: <MapPin /> },
        { path: '/albums', label: '相册', icon: <BookImage /> },
        { path: '/duplicates', label: '重复', icon: <Copy /> },
        { path: '/search', label: '搜索', icon: <Search /> },
    ];

    const managementItems = [
        { path: '/trash', label: '回收站', icon: <Trash2 /> },
        { path: '/settings', label: '设置', icon: <Settings /> },
    ];

    return (
        <div className="app-layout">
            <aside className="sidebar">
                <div className="sidebar-header">
                    <div className="sidebar-logo">
                        <Camera size={18} />
                    </div>
                    <span className="sidebar-title">FastPhoto</span>
                </div>

                <nav className="sidebar-nav">
                    <div className="nav-section">
                        <div className="nav-section-label">浏览</div>
                        {navItems.map((item) => (
                            <button
                                key={item.path}
                                className={`nav-item ${location.pathname === item.path ? 'active' : ''}`}
                                onClick={() => navigate(item.path)}
                            >
                                {item.icon}
                                {item.label}
                            </button>
                        ))}
                    </div>

                    <div className="nav-section">
                        <div className="nav-section-label">管理</div>
                        {managementItems.map((item) => (
                            <button
                                key={item.path}
                                className={`nav-item ${location.pathname === item.path ? 'active' : ''}`}
                                onClick={() => navigate(item.path)}
                            >
                                {item.icon}
                                {item.label}
                            </button>
                        ))}
                    </div>
                </nav>

                <div className="sidebar-footer">
                    <button className="theme-toggle" onClick={toggleTheme} title={theme === 'dark' ? '切换到亮色' : '切换到暗色'}>
                        {theme === 'dark' ? <Sun size={16} /> : <Moon size={16} />}
                    </button>
                    <div className="user-badge" onClick={onLogout} title="退出登录">
                        <div className="user-avatar">
                            {user.username.charAt(0).toUpperCase()}
                        </div>
                        <div className="user-info">
                            <div className="user-name">{user.username}</div>
                            <div className="user-role">{user.role === 'admin' ? '管理员' : '用户'}</div>
                        </div>
                        <LogOut size={16} style={{ color: 'var(--text-muted)' }} />
                    </div>
                </div>
            </aside>

            <main className="main-content">
                {children}
            </main>
        </div>
    );
}
