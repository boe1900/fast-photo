import { useState } from 'react';
import { authApi } from '../api';

interface LoginProps {
    onLogin: (user: { id: number; username: string; role: string }, token: string) => void;
}

export default function Login({ onLogin }: LoginProps) {
    const [username, setUsername] = useState('');
    const [password, setPassword] = useState('');
    const [error, setError] = useState('');
    const [isSetup, setIsSetup] = useState<boolean | null>(null);
    const [loading, setLoading] = useState(false);

    useState(() => {
        authApi.setupStatus().then((res) => {
            setIsSetup(res.data.needs_setup);
        }).catch(() => setIsSetup(false));
    });

    const handleSubmit = async (e: React.FormEvent) => {
        e.preventDefault();
        setError('');
        setLoading(true);

        try {
            const action = isSetup ? authApi.register : authApi.login;
            const res = await action(username, password);
            const data = res.data;
            onLogin(data.user, data.token);
        } catch (err: any) {
            setError(err.response?.data?.error || '登录失败，请重试');
        } finally {
            setLoading(false);
        }
    };

    return (
        <div className="login-page">
            <div className="login-card">
                <h1>FastPhoto</h1>
                <p>
                    {isSetup
                        ? '首次使用，请创建管理员账户'
                        : '登录您的照片管理系统'}
                </p>

                {error && <div className="login-error">{error}</div>}

                <form onSubmit={handleSubmit}>
                    <div className="form-group">
                        <label htmlFor="username">用户名</label>
                        <input
                            id="username"
                            className="form-input"
                            type="text"
                            placeholder="输入用户名"
                            value={username}
                            onChange={(e) => setUsername(e.target.value)}
                            autoFocus
                            required
                        />
                    </div>
                    <div className="form-group">
                        <label htmlFor="password">密码</label>
                        <input
                            id="password"
                            className="form-input"
                            type="password"
                            placeholder="输入密码（至少6位）"
                            value={password}
                            onChange={(e) => setPassword(e.target.value)}
                            minLength={6}
                            required
                        />
                    </div>
                    <button className="login-btn" type="submit" disabled={loading}>
                        {loading ? '处理中...' : isSetup ? '创建管理员' : '登录'}
                    </button>
                </form>
            </div>
        </div>
    );
}
