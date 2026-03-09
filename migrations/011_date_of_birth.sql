-- DPIA leeftijdsverificatie: vervang integer age door date_of_birth.
-- Minimumleeftijd 16 jaar; geboortedatum bewaard als bewijs (AVG art. 8).
ALTER TABLE profiles ADD COLUMN date_of_birth DATE NOT NULL DEFAULT '1970-01-01';
ALTER TABLE profiles ALTER COLUMN date_of_birth DROP DEFAULT;
ALTER TABLE profiles DROP COLUMN age;
