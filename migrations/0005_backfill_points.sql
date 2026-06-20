-- Backfill solve points using the new formula:
--   points = clamp(round10(char_count * 8 + difficulty_rating * 20), 50, 1500)
--
-- CASE arms are pre-computed from the current problem set by Problem::points().
-- Solves for titles not in this list (e.g. removed problems) keep their old value.

UPDATE solves SET points = CASE problem_title
    WHEN 'Quadratic Formula' THEN 420
    WHEN 'Pythagorean Theorem' THEN 190
    WHEN 'Sum of first n Squares' THEN 420
    WHEN 'Law of Cosines' THEN 320
    WHEN 'Legendre''s formula' THEN 400
    WHEN 'Euler''s Identity' THEN 170
    WHEN 'Euler''s Lesser-Known Identity' THEN 200
    WHEN 'Normal Distribution' THEN 590
    WHEN 'Fourier Transform' THEN 560
    WHEN 'Wave Equation' THEN 580
    WHEN 'Navier-Stokes Equation' THEN 1270
    WHEN 'Black-Scholes Equation' THEN 1000
    WHEN 'Relativity' THEN 90
    WHEN 'Chaos Theory' THEN 260
    WHEN 'Definition of the Derivative' THEN 490
    WHEN 'Euler''s Formula for Polyhedra' THEN 120
    WHEN 'Gravitation' THEN 210
    WHEN 'AM-GM' THEN 560
    WHEN 'Stirling''s Approximation' THEN 300
    WHEN 'Stokes'' Theorem' THEN 1150
    WHEN 'Divergence Theorem' THEN 990
    WHEN 'Cauchy-Schwarz Inequality' THEN 1490
    WHEN 'Area of a Circle' THEN 100
    WHEN 'Definition of tau' THEN 100
    WHEN 'Sophie Germain Identity' THEN 520
    WHEN 'Pascal''s Identity' THEN 450
    WHEN 'Hockey-stick Identity' THEN 420
    WHEN 'Vandermonde''s Identity' THEN 510
    WHEN 'Combinations' THEN 300
    WHEN 'Heine''s Identity' THEN 720
    WHEN 'Binomial identity' THEN 450
    WHEN 'Hermite''s Identity' THEN 430
    WHEN 'Matrix Determinant Lemma' THEN 1280
    WHEN 'Euler Product of the Riemann-Zeta function' THEN 610
    WHEN 'Irrationality of the Square Root of 2' THEN 160
    WHEN 'Heron''s Formula' THEN 460
    WHEN 'Continued Fraction for pi/2' THEN 670
    WHEN 'Sophomore''s Dream' THEN 430
    WHEN 'Identity involving pi and e' THEN 520
    WHEN 'Representation of the Golden Ratio' THEN 480
    WHEN 'The Sum of all Positive Integers' THEN 260
    WHEN 'Inverse of a complex number' THEN 410
    WHEN 'Definition of Convolution' THEN 490
    WHEN 'Definition of the Kronecker Delta function' THEN 460
    WHEN 'Bayes'' Theorem' THEN 310
    WHEN 'Probability Density Function of the Student''s t-distribution' THEN 780
    WHEN 'De Morgan''s laws' THEN 350
    WHEN 'Determinant of a 2 times 2 matrix' THEN 390
    WHEN 'Sawtooth Function' THEN 660
    WHEN 'Definition of Graham''s Number' THEN 1010
    WHEN 'Burnside''s Lemma' THEN 410
    WHEN 'Continuum Hypothesis' THEN 680
    WHEN 'Pythagorean Identity' THEN 270
    WHEN 'Double Angle for sin' THEN 320
    WHEN 'Double Angle for cos' THEN 380
    WHEN 'Fermat''s Last Theorem' THEN 450
    WHEN 'Fermat''s Little Theorem' THEN 260
    WHEN 'Euler''s Theorem' THEN 460
    WHEN 'QM-AM-GM-HM Inequality over 3 variables' THEN 840
    WHEN 'Extended Law of Sines' THEN 560
    WHEN 'Integration by Parts' THEN 350
    WHEN 'Definition of Perfect Numbers' THEN 350
    WHEN 'Gaussian Integral' THEN 1500
    WHEN 'Definition of an Integral' THEN 850
    WHEN 'Quantum Fourier transform' THEN 610
    WHEN 'Recursive definition of the Hadamard transform' THEN 890
    WHEN 'Imaginary numbers' THEN 90
    WHEN 'Sum of Cubes' THEN 340
    WHEN 'RSA Decryption Algorithm' THEN 420
    WHEN 'Contraposition' THEN 270
    WHEN 'Equation of a spring' THEN 200
    WHEN 'Sum of reciprocals of partial sums of ℕ' THEN 380
    WHEN 'Binet''s Formula' THEN 420
    WHEN 'Sum of first n Cubes' THEN 370
    WHEN 'The Basel Problem' THEN 320
    WHEN 'Root Mean Square' THEN 620
    WHEN 'The Harmonic Series' THEN 240
    WHEN 'Tupper''s Self-Referential Formula' THEN 640
    WHEN 'Hölder''s Inequality' THEN 840
    WHEN 'Rearrangement Inequality' THEN 1220
    WHEN 'Law of Tangents' THEN 730
    WHEN 'Euler''s Arctangent Identity' THEN 670
    WHEN 'The Dirichlet Convolution' THEN 430
    WHEN 'Sum of a Row of Pascal''s Triangle' THEN 580
    WHEN 'Definitions of Catalan''s Constant' THEN 940
    WHEN 'Series Representation of Apéry''s Constant' THEN 600
    WHEN 'Definition of the Euler-Mascheroni Constant' THEN 820
    WHEN 'Green''s First Identity' THEN 1160
    WHEN 'Cauchy-Riemann Equations' THEN 940
    WHEN 'Cauchy''s Integral Formula' THEN 600
    WHEN 'Cauchy''s Differentiation Formula' THEN 700
    WHEN 'Functional Equation for the Riemann-Zeta Function' THEN 740
    WHEN 'Well-ordering Principle' THEN 960
    WHEN 'Asymptotic Formula for the Dirichlet Divisor Function' THEN 540
    WHEN 'Prime Number Theorem' THEN 220
    WHEN 'Cumulative Distribution Function of the Gaussian Distribution' THEN 540
    WHEN 'Chernoff Bound' THEN 440
    WHEN 'Union Bound' THEN 480
    WHEN 'Law of Total Probability' THEN 350
    WHEN 'Linear Least Squares Estimator' THEN 560
    WHEN 'Definition of the Dilogarithm' THEN 550
    WHEN 'Leibniz''s Determinant Formula' THEN 670
    WHEN 'Euler-Lagrange Equations' THEN 670
    WHEN 'Sum of Divisors' THEN 680
    WHEN 'Einstein Field Equations' THEN 490
    WHEN 'Second Fundamental Theorem of Calculus' THEN 460
    WHEN 'Abel''s Summation Formula' THEN 720
    WHEN 'Lagrange''s Theorem' THEN 240
    WHEN 'Catalan Numbers' THEN 630
    WHEN 'Ising Model Hamiltonian' THEN 760
    WHEN 'Borwein Integral' THEN 810
    WHEN 'Parseval Gutzmer Formula' THEN 1100
    WHEN 'Fubini''s Theorem' THEN 1090
    WHEN 'Coarea Formula' THEN 900
    WHEN 'Equation of a Torus' THEN 320
    WHEN 'Ampère-Maxwell law' THEN 950
    WHEN 'Gauss''s Flux Theorem (differential form)' THEN 420
    WHEN 'Gauss''s law for Magnetism' THEN 280
    WHEN 'Maxwell–Faraday equation' THEN 620
    WHEN 'Eigenvalue Formula' THEN 450
    WHEN 'Collatz Function' THEN 820
    WHEN 'Gamma Function' THEN 420
    WHEN 'Laplace Transform' THEN 420
    WHEN 'Taylor Series' THEN 450
    WHEN 'General Solution to First-Order Linear Differential Equations' THEN 870
    WHEN 'Fibonacci Binomial Coefficients Identity' THEN 940
    WHEN 'Bellman Optimality Equation' THEN 650
    WHEN 'Definition of a Well-founded Relation' THEN 720
    WHEN 'Estimation Lemma' THEN 530
    WHEN 'Chaitin''s Constant' THEN 330
    WHEN 'Cauchy''s Differentiation Formula' THEN 680
    WHEN 'Defintion of the Quasi-Stationary Distribution' THEN 770
    WHEN 'Addition of Sound Levels in Decibels' THEN 480
    WHEN 'Fast-Growing Hierarchy' THEN 930
    WHEN 'Feigenbaum-Cvitanović Functional Equation' THEN 300
    WHEN 'Feynman''s Trick' THEN 620
    WHEN 'Lorentz Factor' THEN 310
    WHEN 'Time Dilation' THEN 400
    WHEN 'Gauss''s Flux Theorem (integral form)' THEN 660
    WHEN 'Doppler Effect' THEN 640
    WHEN 'Bernoulli''s Equation' THEN 740
    WHEN 'Relation between K_p and K_c' THEN 240
    WHEN 'Van der Waals Equation' THEN 340
    WHEN 'Maxwell-Boltzmann Distribution' THEN 600
    WHEN 'Cayley-Hamilton Theorem' THEN 760
    WHEN 'Chudnovsky''s Formula for pi' THEN 960
    WHEN 'Residue Theorem' THEN 880
    WHEN 'The Fundamental Group of the Circle' THEN 240
    WHEN 'Definition of the Operator Norm on a Finite Dimensional Banach Space.' THEN 750
    WHEN 'Green''s Theorem' THEN 1040
    WHEN 'Portfolio Variance' THEN 940
    WHEN 'Newton''s Method' THEN 350
    WHEN 'Shannon Entropy' THEN 400
    WHEN 'Pinsker''s inequality' THEN 400
    WHEN 'Condtional Entropy' THEN 670
    WHEN 'Beta Function' THEN 470
    WHEN 'Moist Adiabatic Lapse Rate' THEN 940
    WHEN 'Cardano''s Formula' THEN 790
    WHEN 'General Cubic Formula' THEN 600
    WHEN 'Tangent Sum of Angles Formula' THEN 850
    WHEN 'Inner Product of Continuous Complex Valued Functions' THEN 600
    WHEN 'Definition of a Psuedorandom Generator' THEN 930
    WHEN 'Generalized Stokes'' theorem' THEN 430
    WHEN 'Cartan''s magic formula' THEN 440
    WHEN 'Ridge Regression' THEN 960
    WHEN 'Evidence Lower Bound (ELBO)' THEN 860
    WHEN 'Langevin Dynamics (Overdamped)' THEN 520
    WHEN 'Schrodinger''s Equation' THEN 380
    WHEN 'Heisenberg''s Uncertainty Principle' THEN 250
    WHEN 'Principle of Inclusion-Exclusion' THEN 430
    WHEN 'General Principle of Inclusion-Exclusion' THEN 1000
    WHEN 'Spectral Decomposition' THEN 1500
    WHEN 'Wigner Transform of the Density Matrix' THEN 940
    WHEN 'Power Mean' THEN 1100
    WHEN 'Alternating Harmonic Series' THEN 450
    WHEN 'Mertens'' therorem' THEN 480
    WHEN 'Rademacher Complexity' THEN 850
    WHEN 'Definition of the Euler Totient Function' THEN 730
    WHEN 'Wigner Semicircle Distribution' THEN 720
    WHEN 'Quaternion Multiplication Formula' THEN 760
    WHEN 'Dirac Equation' THEN 410
    WHEN 'Center of Mass' THEN 740
    WHEN 'Sackur-Tetrode equation' THEN 620
    WHEN 'Force-Potential Relation' THEN 1500
    WHEN 'Riemann Zeta Function' THEN 570
    ELSE points
END;

-- Recompute each score row from its session's updated solve points.
-- Matches session to score row via problems_solved count and nearest timestamp
-- (score is recorded shortly after the last solve in that session).
-- Rows with no matching session (e.g. from before the solves table existed)
-- keep their existing score unchanged via the COALESCE fallback.
UPDATE scores
SET score = COALESCE(
    (
        SELECT SUM(sv.points)
        FROM solves sv
        WHERE sv.session = (
            SELECT g.session
            FROM (
                SELECT session,
                       COUNT(*)        AS cnt,
                       MAX(created_at) AS last_at
                FROM solves
                GROUP BY session
            ) g
            WHERE g.cnt      = scores.problems_solved
              AND g.last_at <= scores.created_at
            ORDER BY g.last_at DESC
            LIMIT 1
        )
    ),
    scores.score
);
