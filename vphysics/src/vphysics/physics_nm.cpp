#include "physics_nm.h"

#include "conversion.h"
#include "physics_collision.h"
#include "physics_constraint.h"
#include "physics_environment.h"
#include "physics_object.h"
#include "physics_surface_props.h"

#include "box3d/box3d.h"

#include <algorithm>
#include <cmath>
#include <memory>
#include <string>
#include <unordered_map>
#include <vector>

static constexpr float SOURCE_TO_IVP_SQUARED = SOURCE_TO_IVP * SOURCE_TO_IVP;
static constexpr float IVP_TO_SOURCE_SQUARED = IVP_TO_SOURCE * IVP_TO_SOURCE;

CPhysicsNM g_PhysicsNM;

static void StoreVector( float *pOut, b3Vec3 v )
{
	pOut[0] = v.x;
	pOut[1] = v.y;
	pOut[2] = v.z;
}

static bool LoadVector( const float *pIn, float flScale, b3Vec3 &v )
{
	v = pIn ? b3Vec3{ pIn[0] * flScale, pIn[1] * flScale, pIn[2] * flScale } : b3Vec3_zero;
	return std::isfinite( v.x ) && std::isfinite( v.y ) && std::isfinite( v.z );
}

// the bodies a constraint NM sets up joins become NM's too
static bool MarkConstraintBodies( IPhysicsConstraint *pConstraint, bool bResult )
{
	if ( bResult )
	{
		CPhysicsConstraint *pPhysicsConstraint = static_cast<CPhysicsConstraint *>( pConstraint );
		for ( CPhysicsObject *pObject : { pPhysicsConstraint->GetReference(), pPhysicsConstraint->GetAttached() } )
		{
			if ( pObject )
				pObject->SetNMDriven();
		}
	}
	return bResult;
}

bool CPhysicsNM::GetBodyState( const IPhysicsObject *pObject, PhysicsNMBodyState_t *pState )
{
	if ( !pObject || !pState )
		return false;

	b3Vec3 center, velocity, angular;
	b3Quat rotation;
	static_cast<const CPhysicsObject *>( pObject )->GetBodyState( center, rotation, velocity, angular );
	StoreVector( pState->m_Position, b3MulSV( SOURCE_TO_IVP, center ) );
	pState->m_Rotation[0] = rotation.v.x;
	pState->m_Rotation[1] = rotation.v.y;
	pState->m_Rotation[2] = rotation.v.z;
	pState->m_Rotation[3] = rotation.s;
	StoreVector( pState->m_Velocity, b3MulSV( SOURCE_TO_IVP, velocity ) );
	StoreVector( pState->m_AngularVelocity, angular );
	return true;
}

bool CPhysicsNM::ApplyBodyDrive( IPhysicsObject *pObject, const float *pForce, const float *pTorque )
{
	if ( !pObject || !pObject->IsMoveable() )
		return false;

	b3Vec3 force, torque;
	if ( !LoadVector( pForce, IVP_TO_SOURCE, force ) || !LoadVector( pTorque, IVP_TO_SOURCE_SQUARED, torque ) )
		return false;

	static_cast<CPhysicsObject *>( pObject )->ApplyDrive( force, torque );
	static_cast<CPhysicsObject *>( pObject )->SetNMDriven();
	return true;
}

bool CPhysicsNM::SetJointArmature( IPhysicsConstraint *pConstraint, float flArmature )
{
	if ( !pConstraint || !( flArmature >= 0.0f ) )
		return false;

	static_cast<CPhysicsConstraint *>( pConstraint )->SetArmature( std::min( flArmature, VPHYSICS_NM_MAX_ARMATURE ) * IVP_TO_SOURCE_SQUARED );
	return MarkConstraintBodies( pConstraint, true );
}

float CPhysicsNM::GetJointArmature( const IPhysicsConstraint *pConstraint )
{
	return pConstraint ? static_cast<const CPhysicsConstraint *>( pConstraint )->GetArmature() * SOURCE_TO_IVP_SQUARED : 0.0f;
}

IPhysicsConstraint *CPhysicsNM::FindConstraint( IPhysicsObject *pObject0, IPhysicsObject *pObject1 )
{
	if ( !pObject0 || !pObject1 )
		return nullptr;

	for ( CPhysicsConstraint *pConstraint : static_cast<CPhysicsObject *>( pObject0 )->GetConstraints() )
	{
		const CPhysicsObject *pReference = pConstraint->GetReference();
		const CPhysicsObject *pAttached = pConstraint->GetAttached();
		if ( ( pReference == pObject0 && pAttached == pObject1 ) || ( pReference == pObject1 && pAttached == pObject0 ) )
			return pConstraint;
	}

	return nullptr;
}

bool CPhysicsNM::SetStepCallback( IPhysicsEnvironment *pEnvironment, PhysicsNMStepFn_t pCallback, void *pContext )
{
	if ( !pEnvironment )
		return false;

	static_cast<CPhysicsEnvironment *>( pEnvironment )->SetNMStepCallback( pCallback, pContext );
	return true;
}

IPhysicsEnvironment *CPhysicsNM::GetObjectEnvironment( const IPhysicsObject *pObject )
{
	return pObject ? static_cast<const CPhysicsObject *>( pObject )->GetEnvironment() : nullptr;
}

int CPhysicsNM::GetSubstepCount()
{
	return CPhysicsEnvironment::SUBSTEP_COUNT;
}

bool CPhysicsNM::SetJointAnchor( IPhysicsConstraint *pConstraint, const float *pWorldPoint )
{
	if ( !pConstraint )
		return false;

	CPhysicsConstraint *pPhysicsConstraint = static_cast<CPhysicsConstraint *>( pConstraint );
	if ( !pWorldPoint )
		return pPhysicsConstraint->SetAnchor( nullptr );

	b3Vec3 point;
	if ( !LoadVector( pWorldPoint, IVP_TO_SOURCE, point ) )
		return false;

	return MarkConstraintBodies( pConstraint, pPhysicsConstraint->SetAnchor( &point ) );
}

static bool LoadQuat( const float *pIn, b3Quat &q )
{
	const float flLength = std::sqrt( ( pIn[0] * pIn[0] + pIn[1] * pIn[1] ) + ( pIn[2] * pIn[2] + pIn[3] * pIn[3] ) );
	if ( !std::isfinite( flLength ) || flLength < 1e-6f )
		return false;

	const float flScale = 1.0f / flLength;
	q = { { pIn[0] * flScale, pIn[1] * flScale, pIn[2] * flScale }, pIn[3] * flScale };
	return true;
}

bool CPhysicsNM::SetJointLimits( IPhysicsConstraint *pConstraint, IPhysicsObject *pParent, const float *pWorldFrames, bool bHinge, const float *pLimits )
{
	if ( !pConstraint || !pParent || !pWorldFrames || !pLimits )
		return false;

	b3Quat frames[2];
	if ( !LoadQuat( pWorldFrames, frames[0] ) || !LoadQuat( pWorldFrames + 4, frames[1] ) )
		return false;

	const float limits[3] = { pLimits[0], pLimits[1], bHinge ? 0.0f : pLimits[2] };
	if ( !std::isfinite( limits[0] ) || !std::isfinite( limits[1] ) || !std::isfinite( limits[2] ) )
		return false;

	return MarkConstraintBodies( pConstraint, static_cast<CPhysicsConstraint *>( pConstraint )->SetJointLimits( static_cast<CPhysicsObject *>( pParent ), frames, bHinge, limits ) );
}

bool CPhysicsNM::SetBodyInertia( IPhysicsObject *pObject, const float *pTensor )
{
	if ( !pObject || !pTensor )
		return false;

	for ( int i = 0; i < 9; ++i )
	{
		if ( !std::isfinite( pTensor[i] ) )
			return false;
	}

	// symmetric to rounding, and positive definite (Sylvester: the leading minors)
	const float *t = pTensor;
	const float flScale = std::max( { std::fabs( t[0] ), std::fabs( t[4] ), std::fabs( t[8] ) } );
	if ( std::fabs( t[1] - t[3] ) > 1e-5f * flScale || std::fabs( t[2] - t[6] ) > 1e-5f * flScale || std::fabs( t[5] - t[7] ) > 1e-5f * flScale )
		return false;

	const double a = t[0], b = 0.5 * ( t[1] + t[3] ), c = 0.5 * ( t[2] + t[6] ), d = t[4], e = 0.5 * ( t[5] + t[7] ), f = t[8];
	if ( !( a > 0.0 ) || !( a * d - b * b > 0.0 ) || !( a * ( d * f - e * e ) - b * ( b * f - e * c ) + c * ( b * e - d * c ) > 0.0 ) )
		return false;

	// columns of the symmetric matrix, in the body's frame (Source's axes, which Box3D's body frame is)
	const b3Matrix3 tensor = { { (float)a, (float)b, (float)c }, { (float)b, (float)d, (float)e }, { (float)c, (float)e, (float)f } };
	static_cast<CPhysicsObject *>( pObject )->SetInertiaTensor( tensor );
	static_cast<CPhysicsObject *>( pObject )->SetNMDriven();
	return true;
}

bool CPhysicsNM::SetJointBallLimits( IPhysicsConstraint *pConstraint, IPhysicsObject *pParent, const float *pWorldFrames, const float *pLimits )
{
	if ( !pConstraint || !pParent || !pWorldFrames || !pLimits )
		return false;

	b3Quat frames[2];
	if ( !LoadQuat( pWorldFrames, frames[0] ) || !LoadQuat( pWorldFrames + 4, frames[1] ) )
		return false;

	const float limits[3] = { pLimits[0], pLimits[1], pLimits[2] };
	if ( !std::isfinite( limits[0] ) || !std::isfinite( limits[1] ) || !std::isfinite( limits[2] ) )
		return false;

	return MarkConstraintBodies( pConstraint, static_cast<CPhysicsConstraint *>( pConstraint )->SetJointBall( static_cast<CPhysicsObject *>( pParent ), frames, limits ) );
}

struct NMSurfaceMaterial_t
{
	bool m_bSet = false;
	float m_flFriction = 0.0f;
	float m_flElasticity = 0.0f;
};

static std::vector<NMSurfaceMaterial_t> s_NMSurfaces;

bool GetNMSurfaceMaterial( uint64_t nSurfaceProp, float &flFriction, float &flElasticity )
{
	if ( nSurfaceProp >= s_NMSurfaces.size() || !s_NMSurfaces[nSurfaceProp].m_bSet )
		return false;

	flFriction = s_NMSurfaces[nSurfaceProp].m_flFriction;
	flElasticity = s_NMSurfaces[nSurfaceProp].m_flElasticity;
	return true;
}

bool CPhysicsNM::SetBodyMaterial( IPhysicsObject *pObject, float flFriction, float flElasticity )
{
	if ( !pObject || !std::isfinite( flFriction ) || !std::isfinite( flElasticity ) || flFriction < 0.0f || flElasticity < 0.0f )
		return false;

	static_cast<CPhysicsObject *>( pObject )->SetNMMaterial( flFriction, flElasticity );
	static_cast<CPhysicsObject *>( pObject )->SetNMDriven();
	return true;
}

bool CPhysicsNM::SetSurfaceMaterial( int nSurfaceProp, float flFriction, float flElasticity )
{
	if ( nSurfaceProp < 0 || nSurfaceProp >= g_PhysicsSurfaceProps.SurfacePropCount() || !std::isfinite( flFriction ) || !std::isfinite( flElasticity ) )
		return false;

	if ( s_NMSurfaces.size() <= static_cast<size_t>( nSurfaceProp ) )
		s_NMSurfaces.resize( nSurfaceProp + 1 );
	s_NMSurfaces[nSurfaceProp] = { flFriction >= 0.0f, flFriction, std::max( flElasticity, 0.0f ) };
	return true;
}

bool CPhysicsNM::GetBodyInertia( const IPhysicsObject *pObject, float *pTensor )
{
	if ( !pObject || !pTensor )
		return false;

	// Box3D's own (kg in^2 in the body's frame, Source's axes)
	const b3MassData mass = b3Body_GetMassData( static_cast<const CPhysicsObject *>( pObject )->GetBody() );
	const b3Vec3 columns[3] = { mass.inertia.cx, mass.inertia.cy, mass.inertia.cz };
	for ( int r = 0; r < 3; ++r )
	{
		for ( int c = 0; c < 3; ++c )
		{
			const b3Vec3 &column = columns[c];
			pTensor[r * 3 + c] = ( r == 0 ? column.x : r == 1 ? column.y : column.z ) * SOURCE_TO_IVP_SQUARED;
		}
	}
	return true;
}
// The hulls CreateBody made, by body: an object only points at its collide, which has to outlive it (null for a
// kept hull, which the next body of the same points takes)
static std::unordered_map<const IPhysicsObject *, CPhysCollide *> s_NMBodies;

// The hulls kept, by their points' bytes: a hull takes far longer to make than a body (VBOX3D_NM_006)
static std::unordered_map<std::string, CPhysCollide *> s_NMHulls;
static constexpr size_t MAX_NM_HULLS = 4096;

bool GetNMCollisionDecision( const CPhysicsObject *pObject0, const CPhysicsObject *pObject1, bool &bCollide )
{
	const NMCollisionSet_t *pSet = pObject0 ? pObject0->GetNMCollisionSet() : nullptr;
	if ( !pSet || !pObject1 || pObject1->GetNMCollisionSet() != pSet )
		return false;

	const int i = pObject0->GetNMCollisionIndex(), j = pObject1->GetNMCollisionIndex();
	if ( i < 0 || j < 0 || i >= pSet->m_nCount || j >= pSet->m_nCount )
		return false;

	bCollide = pSet->m_Collide[i * pSet->m_nCount + j] != 0;
	return true;
}

// A rotation (quaternion x, y, z, w) and a point (Source units) as a matrix3x4_t
static void QuatMatrix( const b3Quat &q, const Vector &origin, matrix3x4_t &m )
{
	const b3Vec3 columns[3] = { b3RotateVector( q, { 1.0f, 0.0f, 0.0f } ), b3RotateVector( q, { 0.0f, 1.0f, 0.0f } ), b3RotateVector( q, { 0.0f, 0.0f, 1.0f } ) };
	for ( int r = 0; r < 3; ++r )
	{
		m[r][0] = r == 0 ? columns[0].x : r == 1 ? columns[0].y : columns[0].z;
		m[r][1] = r == 0 ? columns[1].x : r == 1 ? columns[1].y : columns[1].z;
		m[r][2] = r == 0 ? columns[2].x : r == 1 ? columns[2].y : columns[2].z;
	}
	m[0][3] = origin.x;
	m[1][3] = origin.y;
	m[2][3] = origin.z;
}

// a^-1 * b for rigid transforms
static void RelativeMatrix( const matrix3x4_t &a, const matrix3x4_t &b, matrix3x4_t &out )
{
	for ( int r = 0; r < 3; ++r )
	{
		for ( int c = 0; c < 3; ++c )
			out[r][c] = a[0][r] * b[0][c] + a[1][r] * b[1][c] + a[2][r] * b[2][c];
		const float d[3] = { b[0][3] - a[0][3], b[1][3] - a[1][3], b[2][3] - a[2][3] };
		out[r][3] = a[0][r] * d[0] + a[1][r] * d[1] + a[2][r] * d[2];
	}
}

IPhysicsObject *CPhysicsNM::CreateBody( IPhysicsEnvironment *pEnvironment, IPhysicsObject *pOwner, const float *pPoints, int nPoints, float flMass, const float *pPosition, const float *pRotation, unsigned short nGameFlags )
{
	if ( !pEnvironment || !pPoints || nPoints < 4 || !pPosition || !pRotation || !( flMass > 0.0f ) || !std::isfinite( flMass ) )
		return nullptr;

	b3Vec3 position;
	b3Quat rotation;
	if ( !LoadVector( pPosition, IVP_TO_SOURCE, position ) || !LoadQuat( pRotation, rotation ) )
		return nullptr;

	std::vector<Vector> points( nPoints );
	std::vector<Vector *> pointers( nPoints );
	for ( int i = 0; i < nPoints; ++i )
	{
		b3Vec3 p;
		if ( !LoadVector( pPoints + 3 * i, IVP_TO_SOURCE, p ) )
			return nullptr;
		points[i] = ToSource( p );
		pointers[i] = &points[i];
	}

	std::string key( reinterpret_cast<const char *>( pPoints ), sizeof( float ) * 3 * static_cast<size_t>( nPoints ) );
	auto kept = s_NMHulls.find( key );
	CPhysCollide *pCollide = kept != s_NMHulls.end() ? kept->second : nullptr;
	bool bKept = pCollide != nullptr;
	if ( !pCollide )
	{
		CPhysConvex *pConvex = g_PhysicsCollision.ConvexFromVerts( pointers.data(), nPoints );
		if ( !pConvex )
			return nullptr;

		pCollide = g_PhysicsCollision.ConvertConvexToCollide( &pConvex, 1 );
		if ( !pCollide )
			return nullptr;

		if ( s_NMHulls.size() < MAX_NM_HULLS )
		{
			s_NMHulls.emplace( std::move( key ), pCollide );
			bKept = true;
		}
	}

	matrix3x4_t frame;
	QuatMatrix( rotation, ToSource( position ), frame );

	objectparams_t params = {};
	params.mass = flMass;
	params.inertia = 1.0f;
	params.rotInertiaLimit = 0.05f;
	params.pName = "dms_nm_body";
	params.pGameData = pOwner ? pOwner->GetGameData() : nullptr;
	params.dragCoefficient = 1.0f;
	params.enableCollisions = true;

	IPhysicsObject *pObject = pEnvironment->CreatePolyObject( pCollide, pOwner ? pOwner->GetMaterialIndex() : -1, ToSource( position ), MatrixAngles( frame ), &params );
	if ( !pObject )
	{
		if ( !bKept )
			g_PhysicsCollision.DestroyCollide( pCollide );
		return nullptr;
	}

	if ( pOwner )
	{
		pObject->SetCallbackFlags( pOwner->GetCallbackFlags() );
		pObject->SetGameFlags( pOwner->GetGameFlags() | nGameFlags );
	}
	else
	{
		pObject->SetGameFlags( nGameFlags );
	}
	// the angles' round trip loses a little: the frame as given
	pObject->SetPositionMatrix( frame, true );
	s_NMBodies[pObject] = bKept ? nullptr : pCollide;
	static_cast<CPhysicsObject *>( pObject )->SetNMDriven();
	pObject->Wake();
	return pObject;
}

IPhysicsConstraint *CPhysicsNM::CreateJoint( IPhysicsObject *pParent, IPhysicsObject *pChild, bool bHinge )
{
	if ( !pParent || !pChild || pParent == pChild )
		return nullptr;

	matrix3x4_t parent, child;
	pParent->GetPositionMatrix( &parent );
	pChild->GetPositionMatrix( &child );

	// Defaults() but for SetIdentityMatrix, which is mathlib's (not linked)
	constraint_ragdollparams_t ragdoll = {};
	ragdoll.constraint.Defaults();
	ragdoll.isActive = true;
	ragdoll.parentIndex = ragdoll.childIndex = -1;
	ragdoll.constraintToReference[0][0] = ragdoll.constraintToReference[1][1] = ragdoll.constraintToReference[2][2] = 1.0f;
	// the constraint's frame is the child's (the reference's) as it is now
	RelativeMatrix( parent, child, ragdoll.constraintToAttached );
	for ( int i = 0; i < 3; ++i )
		ragdoll.axes[i].SetAxisFriction( bHinge && i != 2 ? 0.0f : -170.0f, bHinge && i != 2 ? 0.0f : 170.0f, 0.0f );

	CPhysicsEnvironment *pEnvironment = static_cast<CPhysicsObject *>( pChild )->GetEnvironment();
	IPhysicsConstraint *pConstraint = pEnvironment->CreateRagdollConstraint( pChild, pParent, nullptr, ragdoll );
	return MarkConstraintBodies( pConstraint, pConstraint != nullptr ) ? pConstraint : nullptr;
}

void CPhysicsNM::DestroyJoint( IPhysicsConstraint *pConstraint )
{
	if ( !pConstraint )
		return;

	CPhysicsConstraint *pPhysicsConstraint = static_cast<CPhysicsConstraint *>( pConstraint );
	CPhysicsObject *pObject = pPhysicsConstraint->GetReference() ? pPhysicsConstraint->GetReference() : pPhysicsConstraint->GetAttached();
	if ( pObject )
		pObject->GetEnvironment()->DestroyConstraint( pConstraint );
}

void CPhysicsNM::DestroyBody( IPhysicsObject *pObject )
{
	if ( !pObject )
		return;

	auto it = s_NMBodies.find( pObject );
	CPhysCollide *pCollide = it != s_NMBodies.end() ? it->second : nullptr;
	if ( it != s_NMBodies.end() )
		s_NMBodies.erase( it );

	static_cast<CPhysicsObject *>( pObject )->GetEnvironment()->DestroyObject( pObject );
	if ( pCollide )
		g_PhysicsCollision.DestroyCollide( pCollide );
}

bool CPhysicsNM::SetBodyCollisionPairs( IPhysicsObject **ppObjects, int nObjects, const int *pPairs, int nPairs )
{
	if ( !ppObjects || nObjects < 2 || nPairs < 0 || ( nPairs > 0 && !pPairs ) )
		return false;

	for ( int i = 0; i < nObjects; ++i )
	{
		if ( !ppObjects[i] || std::find( ppObjects, ppObjects + i, ppObjects[i] ) != ppObjects + i )
			return false;
	}

	auto pSet = std::make_shared<NMCollisionSet_t>();
	pSet->m_nCount = nObjects;
	pSet->m_Collide.assign( static_cast<size_t>( nObjects ) * nObjects, 0 );
	for ( int k = 0; k < nPairs; ++k )
	{
		const int a = pPairs[2 * k], b = pPairs[2 * k + 1];
		if ( a < 0 || b < 0 || a >= nObjects || b >= nObjects )
			return false;
		pSet->m_Collide[a * nObjects + b] = pSet->m_Collide[b * nObjects + a] = 1;
	}

	std::shared_ptr<const NMCollisionSet_t> pShared = pSet;
	for ( int i = 0; i < nObjects; ++i )
		static_cast<CPhysicsObject *>( ppObjects[i] )->SetNMCollisionSet( pShared, i );
	// the filter's cached decisions about them are stale
	for ( int i = 0; i < nObjects; ++i )
		ppObjects[i]->RecheckCollisionFilter();
	return true;
}

bool CPhysicsNM::SetBodyState( IPhysicsObject *pObject, const PhysicsNMBodyState_t *pState )
{
	if ( !pObject || !pState || pObject->IsStatic() || !pObject->IsMotionEnabled() )
		return false;

	b3Vec3 center, velocity, angular;
	b3Quat rotation;
	if ( !LoadVector( pState->m_Position, IVP_TO_SOURCE, center ) || !LoadQuat( pState->m_Rotation, rotation ) || !LoadVector( pState->m_Velocity, IVP_TO_SOURCE, velocity ) || !LoadVector( pState->m_AngularVelocity, 1.0f, angular ) )
		return false;

	// the frame's origin: the centre of mass less its offset in the body's frame (Source units)
	const Vector local = pObject->GetMassCenterLocalSpace();
	const b3Vec3 origin = b3Sub( center, b3RotateVector( rotation, { local.x, local.y, local.z } ) );
	matrix3x4_t frame;
	QuatMatrix( rotation, ToSource( origin ), frame );
	pObject->SetPositionMatrix( frame, true );

	// IPhysicsObject's: units/s, and degrees/s about the body's own axes
	const Vector v = ToSource( velocity );
	const b3Vec3 w = b3InvRotateVector( rotation, angular );
	const float flDegrees = 180.0f / 3.14159265358979f;
	const AngularImpulse spin( w.x * flDegrees, w.y * flDegrees, w.z * flDegrees );
	pObject->SetVelocityInstantaneous( &v, &spin );
	pObject->Wake();
	return true;
}