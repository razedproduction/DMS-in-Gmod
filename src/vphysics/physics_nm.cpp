#include "physics_nm.h"

#include "conversion.h"
#include "physics_constraint.h"
#include "physics_environment.h"
#include "physics_object.h"

#include "box3d/box3d.h"

#include <algorithm>
#include <cmath>

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
	return true;
}

bool CPhysicsNM::SetJointArmature( IPhysicsConstraint *pConstraint, float flArmature )
{
	if ( !pConstraint || !( flArmature >= 0.0f ) )
		return false;

	static_cast<CPhysicsConstraint *>( pConstraint )->SetArmature( std::min( flArmature, VPHYSICS_NM_MAX_ARMATURE ) * IVP_TO_SOURCE_SQUARED );
	return true;
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

	return pPhysicsConstraint->SetAnchor( &point );
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

	return static_cast<CPhysicsConstraint *>( pConstraint )->SetJointLimits( static_cast<CPhysicsObject *>( pParent ), frames, bHinge, limits );
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

	return static_cast<CPhysicsConstraint *>( pConstraint )->SetJointBall( static_cast<CPhysicsObject *>( pParent ), frames, limits );
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