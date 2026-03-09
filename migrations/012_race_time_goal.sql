-- Tijdsdoelstelling voor de gekozen race (bijv. "3:45:00"), optioneel
ALTER TABLE profiles ADD COLUMN IF NOT EXISTS race_time_goal TEXT;
