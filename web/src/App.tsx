import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { useState } from 'react';
import Login from './pages/Login';
import Layout from './components/Layout';
import Timeline from './pages/Timeline';
import Folders from './pages/Folders';
import Search from './pages/Search';
import Explore from './pages/Explore';
import MapView from './pages/MapView';
import Albums from './pages/Albums';
import People from './pages/People';
import Favorites from './pages/Favorites';
import Trash from './pages/Trash';
import Duplicates from './pages/Duplicates';
import Settings from './pages/Settings';
import './App.css';

interface User {
  id: number;
  username: string;
  role: string;
}

function getStoredUser(): User | null {
  const token = localStorage.getItem('token');
  const saved = localStorage.getItem('user');
  if (!token || !saved) {
    return null;
  }

  try {
    return JSON.parse(saved) as User;
  } catch {
    return null;
  }
}

function App() {
  const [user, setUser] = useState<User | null>(() => getStoredUser());

  const handleLogin = (u: User, token: string) => {
    localStorage.setItem('token', token);
    localStorage.setItem('user', JSON.stringify(u));
    setUser(u);
  };

  const handleLogout = () => {
    localStorage.removeItem('token');
    localStorage.removeItem('user');
    setUser(null);
  };

  return (
    <BrowserRouter>
      <Routes>
        <Route
          path="/login"
          element={
            user ? <Navigate to="/" /> : <Login onLogin={handleLogin} />
          }
        />
        <Route
          path="/*"
          element={
            user ? (
              <Layout user={user} onLogout={handleLogout}>
                <Routes>
                  <Route path="/" element={<Timeline />} />
                  <Route path="/folders" element={<Folders />} />
                  <Route path="/explore" element={<Explore />} />
                  <Route path="/map" element={<MapView />} />
                  <Route path="/albums" element={<Albums />} />
                  <Route path="/people" element={<People />} />
                  <Route path="/favorites" element={<Favorites />} />
                  <Route path="/trash" element={<Trash />} />
                  <Route path="/duplicates" element={<Duplicates />} />
                  <Route path="/search" element={<Search />} />
                  <Route path="/settings" element={<Settings />} />
                </Routes>
              </Layout>
            ) : (
              <Navigate to="/login" />
            )
          }
        />
      </Routes>
    </BrowserRouter>
  );
}

export default App;
